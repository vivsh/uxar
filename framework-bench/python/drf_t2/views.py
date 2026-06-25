import asyncio
import json
import os
import threading
import time
from collections import defaultdict

import psycopg
from django.http import JsonResponse, StreamingHttpResponse
from rest_framework.decorators import api_view
from rest_framework.response import Response

DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:postgres@localhost:5432/vyuh_bench")
queues: dict[int, set[asyncio.Queue]] = defaultdict(set)
loop = asyncio.new_event_loop()


def conn():
    return psycopg.connect(DATABASE_URL)


def migrate() -> None:
    sql = open(os.path.join(os.path.dirname(__file__), "../../sql/t2_postgres.sql")).read()
    with conn() as db:
        db.execute(sql)


def publish(project_id: int, kind: str, value: int) -> None:
    event = {"project_id": project_id, "kind": kind, "value": value, "at_ms": int(time.time() * 1000)}
    for queue in list(queues[project_id]):
        loop.call_soon_threadsafe(queue.put_nowait, event)


@api_view(["GET"])
def health(request):
    return Response({"ok": True})


@api_view(["POST"])
def create_project(request):
    with conn() as db:
        row = db.execute("INSERT INTO projects (name) VALUES (%s) RETURNING id", (request.data["name"],)).fetchone()
    return Response({"id": row[0], "name": request.data["name"]})


@api_view(["GET", "POST"])
def project_events(request, project_id: int):
    if request.method == "POST":
        value = int(request.data["value"])
        with conn() as db:
            row = db.execute(
                "INSERT INTO events (project_id, value) VALUES (%s, %s) RETURNING id",
                (project_id, value),
            ).fetchone()
        publish(project_id, "event", value)
        return Response({"id": row[0]})

    with conn() as db:
        rows = db.execute(
            "SELECT id, value FROM events WHERE project_id = %s ORDER BY id DESC LIMIT 100",
            (project_id,),
        ).fetchall()
    return Response([{"id": row[0], "value": row[1]} for row in rows])


@api_view(["GET"])
def summary(request, project_id: int):
    with conn() as db:
        row = db.execute("SELECT event_count, event_sum FROM rollups WHERE project_id = %s", (project_id,)).fetchone()
    if row is None:
        return Response({"project_id": project_id, "event_count": 0, "event_sum": 0})
    return Response({"project_id": project_id, "event_count": row[0], "event_sum": row[1]})


def stream(request, project_id: int):
    queue: asyncio.Queue = asyncio.Queue(maxsize=256)
    queues[project_id].add(queue)

    def gen():
        try:
            while True:
                event = asyncio.run_coroutine_threadsafe(queue.get(), loop).result()
                yield f"data: {json.dumps(event)}\n\n"
        finally:
            queues[project_id].discard(queue)

    return StreamingHttpResponse(gen(), content_type="text/event-stream")


def poll(request, project_id: int):
    queue: asyncio.Queue = asyncio.Queue(maxsize=1)
    queues[project_id].add(queue)
    try:
        future = asyncio.run_coroutine_threadsafe(queue.get(), loop)
        event = future.result(timeout=25)
        return JsonResponse({"after": request.GET.get("after"), "events": [event]})
    except Exception:
        return JsonResponse({"after": request.GET.get("after"), "events": []})
    finally:
        queues[project_id].discard(queue)


def rollup_loop() -> None:
    while True:
        time.sleep(5)
        with conn() as db:
            rows = db.execute(
                """INSERT INTO rollups (project_id, event_count, event_sum, updated_at)
                   SELECT project_id, COUNT(*), COALESCE(SUM(value), 0), now() FROM events GROUP BY project_id
                   ON CONFLICT (project_id) DO UPDATE SET event_count = EXCLUDED.event_count,
                   event_sum = EXCLUDED.event_sum, updated_at = EXCLUDED.updated_at
                   RETURNING project_id, event_count"""
            ).fetchall()
        for row in rows:
            publish(row[0], "rollup", row[1])


migrate()
threading.Thread(target=loop.run_forever, daemon=True).start()
threading.Thread(target=rollup_loop, daemon=True).start()
