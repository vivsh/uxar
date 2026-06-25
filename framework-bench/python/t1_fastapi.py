import os
import sqlite3
from pathlib import Path

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

DB = os.environ.get("T1_SQLITE_PATH", "/tmp/vyuh_bench_t1_fastapi.sqlite3")
app = FastAPI()


class Echo(BaseModel):
    message: str
    count: int


def conn() -> sqlite3.Connection:
    db = sqlite3.connect(DB)
    db.row_factory = sqlite3.Row
    return db


@app.on_event("startup")
def startup() -> None:
    Path(DB).parent.mkdir(parents=True, exist_ok=True)
    with conn() as db:
        db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        db.executemany("INSERT OR IGNORE INTO items (id, name) VALUES (?, ?)", [(1, "alpha"), (2, "beta"), (3, "gamma")])


@app.get("/health")
def health() -> dict[str, bool]:
    return {"ok": True}


@app.post("/echo")
def echo(input: Echo) -> Echo:
    return input


@app.get("/items/{item_id}")
def item(item_id: int) -> dict[str, object]:
    with conn() as db:
        row = db.execute("SELECT id, name FROM items WHERE id = ?", (item_id,)).fetchone()
    if row is None:
        raise HTTPException(status_code=404, detail="item not found")
    return {"id": row["id"], "name": row["name"]}
