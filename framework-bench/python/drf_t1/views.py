import os
import sqlite3
from pathlib import Path

from rest_framework.decorators import api_view
from rest_framework.response import Response

DB = os.environ.get("T1_SQLITE_PATH", "/tmp/vyuh_bench_t1_drf.sqlite3")


def conn() -> sqlite3.Connection:
    db = sqlite3.connect(DB)
    db.row_factory = sqlite3.Row
    return db


def startup() -> None:
    Path(DB).parent.mkdir(parents=True, exist_ok=True)
    with conn() as db:
        db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        db.executemany("INSERT OR IGNORE INTO items (id, name) VALUES (?, ?)", [(1, "alpha"), (2, "beta"), (3, "gamma")])


@api_view(["GET"])
def health(request):
    return Response({"ok": True})


@api_view(["POST"])
def echo(request):
    return Response(request.data)


@api_view(["GET"])
def item(request, item_id: int):
    with conn() as db:
        row = db.execute("SELECT id, name FROM items WHERE id = ?", (item_id,)).fetchone()
    if row is None:
        return Response({"detail": "item not found"}, status=404)
    return Response({"id": row["id"], "name": row["name"]})


startup()

