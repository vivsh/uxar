"""
Getting Started — Notes API  (FastAPI / Python equivalent)

Equivalent to uxar/examples/notes.rs — same endpoints, same JWT cookie auth,
same role model, same cron job, same auto-generated OpenAPI docs.

Install:
    pip install fastapi uvicorn sqlalchemy asyncpg python-jose apscheduler

Run:
    DATABASE_URL=postgresql+asyncpg://user:pass@localhost/notes_db \
    SECRET_KEY=change-me-in-production \
    uvicorn notes_fastapi:app --port 8080

OpenAPI docs: http://localhost:8080/docs
"""

import logging
import os
from datetime import datetime, timedelta, timezone
from typing import Annotated

from apscheduler.schedulers.asyncio import AsyncIOScheduler
from fastapi import Depends, FastAPI, HTTPException, Response, status
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
from jose import JWTError, jwt
from pydantic import BaseModel
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

# ── Config ────────────────────────────────────────────────────────────────────

DATABASE_URL = os.environ["DATABASE_URL"]
SECRET_KEY   = os.environ["SECRET_KEY"]
ALGORITHM    = "HS256"
ACCESS_TTL   = 3600
REFRESH_TTL  = 604800

# ── Roles ─────────────────────────────────────────────────────────────────────

class Role:
    User  = 1 << 0
    Admin = 1 << 1

# ── Database ──────────────────────────────────────────────────────────────────

engine       = create_async_engine(DATABASE_URL)
SessionLocal = async_sessionmaker(engine, expire_on_commit=False)

async def get_db():
    async with SessionLocal() as session:
        yield session

DB = Annotated[AsyncSession, Depends(get_db)]

# ── Models ────────────────────────────────────────────────────────────────────

class Note(BaseModel):
    id: int; owner: str; title: str; body: str

class NoteInput(BaseModel):
    title: str; body: str

class LoginReq(BaseModel):
    username: str; password: str

# ── Auth ──────────────────────────────────────────────────────────────────────

bearer = HTTPBearer(auto_error=False)

def make_token(sub: str, roles: int, ttl: int) -> str:
    payload = {"sub": sub, "roles": roles, "exp": datetime.now(timezone.utc) + timedelta(seconds=ttl)}
    return jwt.encode(payload, SECRET_KEY, algorithm=ALGORITHM)

def current_user(creds: Annotated[HTTPAuthorizationCredentials | None, Depends(bearer)]) -> dict:
    if not creds:
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Missing token")
    try:
        return jwt.decode(creds.credentials, SECRET_KEY, algorithms=[ALGORITHM])
    except JWTError:
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Invalid token")

def require(role: int):
    def guard(user: Annotated[dict, Depends(current_user)]) -> dict:
        if not (user.get("roles", 0) & role):
            raise HTTPException(status.HTTP_403_FORBIDDEN, "Forbidden")
        return user
    return guard

User  = Annotated[dict, Depends(require(Role.User))]
Admin = Annotated[dict, Depends(require(Role.Admin))]

# ── App ───────────────────────────────────────────────────────────────────────

app = FastAPI(title="Notes API", description="Getting-started example", version="0.1.0")

# ── Handlers ──────────────────────────────────────────────────────────────────

@app.post("/v1/login")
async def login(req: LoginReq, response: Response):
    """Authenticate; sets JWT access + refresh cookies on success."""
    # TODO: verify against your users table with a hashed password check.
    if req.username != "alice" or req.password != "secret":
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Invalid credentials")
    access  = make_token(req.username, Role.User, ACCESS_TTL)
    refresh = make_token(req.username, Role.User, REFRESH_TTL)
    response.set_cookie("access_token",  access,  httponly=True, samesite="lax",    max_age=ACCESS_TTL)
    response.set_cookie("refresh_token", refresh, httponly=True, samesite="strict", max_age=REFRESH_TTL)

@app.get("/v1/notes", response_model=list[Note])
async def list_notes(user: User, db: DB):
    """List all notes belonging to the authenticated user."""
    rows = await db.execute(
        text("SELECT id, owner, title, body FROM notes WHERE owner = :owner"),
        {"owner": user["sub"]},
    )
    return [Note(**dict(r._mapping)) for r in rows.fetchall()]

@app.post("/v1/notes", response_model=Note)
async def create_note(input: NoteInput, user: User, db: DB):
    """Create a note; returns the saved note with its id."""
    row = (await db.execute(
        text("INSERT INTO notes (owner, title, body) VALUES (:owner, :title, :body) RETURNING *"),
        {"owner": user["sub"], "title": input.title, "body": input.body},
    )).fetchone()
    await db.commit()
    return Note(**dict(row._mapping))

@app.delete("/v1/notes/all")
async def purge_notes(_: Admin, db: DB):
    """Delete all notes. Requires Admin role."""
    result = await db.execute(text("DELETE FROM notes"))
    await db.commit()
    return {"deleted": result.rowcount}

# ── Cron ──────────────────────────────────────────────────────────────────────

scheduler = AsyncIOScheduler()

@scheduler.scheduled_job("cron", hour=0, minute=0, second=0)
async def nightly_prune():
    """Fire every night at midnight (extend this to run cleanup queries)."""
    logging.info("nightly prune fired", extra={"triggered_by": "cron"})

@app.on_event("startup")
async def startup():  scheduler.start()

@app.on_event("shutdown")
async def shutdown(): scheduler.shutdown()
