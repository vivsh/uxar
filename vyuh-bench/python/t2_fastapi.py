import asyncio
import os
import time
from collections import defaultdict

import asyncpg
from fastapi import FastAPI
from pydantic import BaseModel
from sse_starlette.sse import EventSourceResponse

DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:postgres@localhost:5432/vyuh_bench")
app = FastAPI()
queues: dict[int, set[asyncio.Queue]] = defaultdict(set)
pool: asyncpg.Pool | None = None


class ProjectIn(BaseModel):
    name: str


class EventIn(BaseModel):
    value: int


class Health(BaseModel):
    ok: bool


class ProjectOut(BaseModel):
    id: int
    name: str


class IdOut(BaseModel):
    id: int


class EventOut(BaseModel):
    id: int
    value: int


class LiveEvent(BaseModel):
    project_id: int
    kind: str
    value: int
    at_ms: int


class SummaryOut(BaseModel):
    project_id: int
    event_count: int
    event_sum: int


class PollOut(BaseModel):
    after: int | None
    events: list[LiveEvent]


async def publish(project_id: int, kind: str, value: int) -> None:
    event = LiveEvent(project_id=project_id, kind=kind, value=value, at_ms=int(time.time() * 1000))
    for queue in list(queues[project_id]):
        queue.put_nowait(event)


@app.on_event("startup")
async def startup() -> None:
    global pool
    pool = await asyncpg.create_pool(DATABASE_URL, min_size=1, max_size=10)
    asyncio.create_task(rollup_loop())


@app.get("/health", response_model=Health)
async def health() -> Health:
    return Health(ok=True)


@app.post("/projects", response_model=ProjectOut)
async def create_project(input: ProjectIn) -> ProjectOut:
    row = await pool.fetchrow("INSERT INTO projects (name) VALUES ($1) RETURNING id, name", input.name)
    return ProjectOut(id=row["id"], name=row["name"])


@app.post("/projects/{project_id}/events", response_model=IdOut)
async def create_event(project_id: int, input: EventIn) -> IdOut:
    row = await pool.fetchrow("INSERT INTO events (project_id, value) VALUES ($1, $2) RETURNING id", project_id, input.value)
    await publish(project_id, "event", input.value)
    return IdOut(id=row["id"])


@app.get("/projects/{project_id}/summary", response_model=SummaryOut)
async def summary(project_id: int) -> SummaryOut:
    row = await pool.fetchrow("SELECT event_count, event_sum FROM rollups WHERE project_id = $1", project_id)
    if row is None:
        return SummaryOut(project_id=project_id, event_count=0, event_sum=0)
    return SummaryOut(project_id=project_id, event_count=row["event_count"], event_sum=row["event_sum"])


@app.get("/projects/{project_id}/events", response_model=list[EventOut])
async def events(project_id: int) -> list[EventOut]:
    rows = await pool.fetch("SELECT id, value FROM events WHERE project_id = $1 ORDER BY id DESC LIMIT 100", project_id)
    return [EventOut(id=row["id"], value=row["value"]) for row in rows]


@app.get("/projects/{project_id}/stream")
async def stream(project_id: int):
    queue: asyncio.Queue = asyncio.Queue(maxsize=256)
    queues[project_id].add(queue)

    async def gen():
        try:
            while True:
                yield {"data": (await queue.get()).model_dump_json()}
        finally:
            queues[project_id].discard(queue)

    return EventSourceResponse(gen())


@app.get("/projects/{project_id}/poll", response_model=PollOut)
async def poll(project_id: int, after: int | None = None) -> PollOut:
    queue: asyncio.Queue = asyncio.Queue(maxsize=1)
    queues[project_id].add(queue)
    try:
        event = await asyncio.wait_for(queue.get(), timeout=25)
        return PollOut(after=after, events=[event])
    except asyncio.TimeoutError:
        return PollOut(after=after, events=[])
    finally:
        queues[project_id].discard(queue)


async def rollup_loop() -> None:
    while True:
        await asyncio.sleep(5)
        rows = await pool.fetch(
            """INSERT INTO rollups (project_id, event_count, event_sum, updated_at)
               SELECT project_id, COUNT(*), COALESCE(SUM(value), 0), now() FROM events GROUP BY project_id
               ON CONFLICT (project_id) DO UPDATE SET event_count = EXCLUDED.event_count,
               event_sum = EXCLUDED.event_sum, updated_at = EXCLUDED.updated_at
               RETURNING project_id, event_count"""
        )
        for row in rows:
            await publish(row["project_id"], "rollup", row["event_count"])
