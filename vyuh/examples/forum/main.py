from __future__ import annotations
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Annotated, Optional

import uvicorn
from fastapi import Depends, FastAPI, HTTPException, Query, status
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
from jose import JWTError, jwt
from passlib.context import CryptContext
from pydantic import BaseModel, EmailStr

DB_PATH   = Path(__file__).parent / "forum.db"
SECRET    = "change-me-in-production-must-be-32-chars+"
ALGORITHM = "HS256"
TTL_SECS  = 86400

pwd    = CryptContext(schemes=["bcrypt"], deprecated="auto")
bearer = HTTPBearer(auto_error=False)

def connect():
    c = sqlite3.connect(DB_PATH, check_same_thread=False)
    c.row_factory = sqlite3.Row
    c.execute("PRAGMA foreign_keys = ON")
    return c

@contextmanager
def db():
    c = connect()
    try:
        yield c
        c.commit()
    except Exception:
        c.rollback()
        raise
    finally:
        c.close()

def migrate():
    with db() as c:
        c.executescript(
            "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, username TEXT NOT NULL UNIQUE, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));"
            "CREATE TABLE IF NOT EXISTS threads (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));"
            "CREATE TABLE IF NOT EXISTS posts (id INTEGER PRIMARY KEY AUTOINCREMENT, thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE, author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));"
            "CREATE TABLE IF NOT EXISTS likes (post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, PRIMARY KEY (post_id, user_id));"
        )

def make_token(uid: int) -> str:
    now = datetime.now(timezone.utc)
    return jwt.encode({"sub": str(uid), "exp": now.timestamp() + TTL_SECS}, SECRET, ALGORITHM)

def require_auth(c: HTTPAuthorizationCredentials | None = Depends(bearer)) -> int:
    if not c:
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Not authenticated")
    try:
        return int(jwt.decode(c.credentials, SECRET, algorithms=[ALGORITHM])["sub"])
    except (JWTError, KeyError, ValueError):
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Invalid token")

Auth = Annotated[int, Depends(require_auth)]

class RegisterIn(BaseModel):
    username: str; email: EmailStr; password: str

class LoginIn(BaseModel):
    username: str; password: str

class ThreadIn(BaseModel):
    title: str

class PostIn(BaseModel):
    body: str

app = FastAPI(title="Forum")

@app.on_event("startup")
def _(): migrate()

@app.post("/auth/register", status_code=201)
def register(b: RegisterIn):
    with db() as c:
        try:
            r = c.execute("INSERT INTO users (username, email, password_hash) VALUES (?,?,?)",
                          (b.username, b.email, pwd.hash(b.password)))
            return {"id": r.lastrowid, "username": b.username}
        except sqlite3.IntegrityError:
            raise HTTPException(409, "Username or email taken")

@app.post("/auth/login")
def login(b: LoginIn):
    with db() as c:
        r = c.execute("SELECT id, password_hash FROM users WHERE username=?", (b.username,)).fetchone()
    if not r or not pwd.verify(b.password, r["password_hash"]):
        raise HTTPException(401, "Invalid credentials")
    return {"access_token": make_token(r["id"]), "token_type": "bearer"}

@app.get("/threads")
def list_threads(search: Optional[str] = None, limit: int = Query(50, ge=1, le=100), offset: int = 0):
    like = f"%{search}%" if search else None
    with db() as c:
        rows = c.execute("""
            SELECT t.id, t.title, t.author_id, u.username AS author,
                   (SELECT COUNT(*) FROM posts p WHERE p.thread_id=t.id) AS post_count,
                   t.created_at, t.updated_at
            FROM threads t JOIN users u ON u.id=t.author_id
            WHERE (? IS NULL OR t.title LIKE ?) ORDER BY t.created_at DESC LIMIT ? OFFSET ?
        """, (like, like, limit, offset)).fetchall()
    return [dict(r) for r in rows]

@app.post("/threads", status_code=201)
def create_thread(b: ThreadIn, uid: Auth):
    with db() as c:
        r = c.execute("INSERT INTO threads (title, author_id) VALUES (?,?)", (b.title, uid))
        return {"id": r.lastrowid}

@app.get("/threads/{id}")
def get_thread(id: int):
    with db() as c:
        r = c.execute("""
            SELECT t.id, t.title, t.author_id, u.username AS author,
                   (SELECT COUNT(*) FROM posts p WHERE p.thread_id=t.id) AS post_count,
                   t.created_at, t.updated_at
            FROM threads t JOIN users u ON u.id=t.author_id WHERE t.id=?""", (id,)).fetchone()
    if not r: raise HTTPException(404, "Thread not found")
    return dict(r)

@app.put("/threads/{id}", status_code=204)
def update_thread(id: int, b: ThreadIn, uid: Auth):
    with db() as c:
        r = c.execute("UPDATE threads SET title=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND author_id=?",
                      (b.title, id, uid))
    if r.rowcount == 0: raise HTTPException(403, "Not found or not your thread")

@app.delete("/threads/{id}", status_code=204)
def delete_thread(id: int, uid: Auth):
    with db() as c:
        r = c.execute("DELETE FROM threads WHERE id=? AND author_id=?", (id, uid))
    if r.rowcount == 0: raise HTTPException(403, "Not found or not your thread")

@app.get("/threads/{tid}/posts")
def list_posts(tid: int, limit: int = Query(50, ge=1, le=100), offset: int = 0):
    with db() as c:
        rows = c.execute("""
            SELECT p.id, p.thread_id, p.author_id, u.username AS author, p.body,
                   (SELECT COUNT(*) FROM likes l WHERE l.post_id=p.id) AS like_count,
                   p.created_at, p.updated_at
            FROM posts p JOIN users u ON u.id=p.author_id
            WHERE p.thread_id=? ORDER BY p.created_at ASC LIMIT ? OFFSET ?
        """, (tid, limit, offset)).fetchall()
    return [dict(r) for r in rows]

@app.post("/threads/{tid}/posts", status_code=201)
def create_post(tid: int, b: PostIn, uid: Auth):
    with db() as c:
        if not c.execute("SELECT 1 FROM threads WHERE id=?", (tid,)).fetchone():
            raise HTTPException(404, "Thread not found")
        r = c.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?,?,?)", (tid, uid, b.body))
        return {"id": r.lastrowid}

@app.put("/posts/{id}", status_code=204)
def update_post(id: int, b: PostIn, uid: Auth):
    with db() as c:
        r = c.execute("UPDATE posts SET body=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND author_id=?",
                      (b.body, id, uid))
    if r.rowcount == 0: raise HTTPException(403, "Not found or not your post")

@app.delete("/posts/{id}", status_code=204)
def delete_post(id: int, uid: Auth):
    with db() as c:
        r = c.execute("DELETE FROM posts WHERE id=? AND author_id=?", (id, uid))
    if r.rowcount == 0: raise HTTPException(403, "Not found or not your post")

@app.post("/posts/{id}/like", status_code=204)
def add_like(id: int, uid: Auth):
    with db() as c:
        c.execute("INSERT OR IGNORE INTO likes (post_id, user_id) VALUES (?,?)", (id, uid))

@app.delete("/posts/{id}/like", status_code=204)
def remove_like(id: int, uid: Auth):
    with db() as c:
        c.execute("DELETE FROM likes WHERE post_id=? AND user_id=?", (id, uid))

@app.get("/posts/{id}/like")
def like_status(id: int):
    with db() as c:
        r = c.execute("SELECT COUNT(*) AS count FROM likes WHERE post_id=?", (id,)).fetchone()
    return {"count": r["count"]}

if __name__ == "__main__":
    uvicorn.run("main:app", host="0.0.0.0", port=8000, reload=True)
