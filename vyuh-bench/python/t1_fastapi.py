import os
import sqlite3

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

DB = os.environ.get("T1_SQLITE_PATH", "/tmp/vyuh_bench_t1_fastapi.sqlite3")
app = FastAPI()


class Echo(BaseModel):
    message: str
    count: int


class Health(BaseModel):
    ok: bool


class Item(BaseModel):
    id: int
    name: str


def conn() -> sqlite3.Connection:
    db = sqlite3.connect(DB)
    db.row_factory = sqlite3.Row
    return db


@app.get("/health", response_model=Health)
def health() -> Health:
    return Health(ok=True)


@app.post("/echo", response_model=Echo)
def echo(input: Echo) -> Echo:
    return input


@app.get("/items/{item_id}", response_model=Item)
def item(item_id: int) -> Item:
    with conn() as db:
        row = db.execute("SELECT id, name FROM items WHERE id = ?", (item_id,)).fetchone()
    if row is None:
        raise HTTPException(status_code=404, detail="item not found")
    return Item(id=row["id"], name=row["name"])
