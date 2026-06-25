-- ============================================================
-- Forum schema (SQLite)
-- ============================================================

CREATE TABLE IF NOT EXISTS users (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    username       TEXT    NOT NULL UNIQUE,
    email          TEXT    NOT NULL UNIQUE,
    password_hash  TEXT    NOT NULL,
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS threads (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT    NOT NULL,
    author_id   INTEGER NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS posts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id   INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    author_id   INTEGER NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    body        TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Composite PK enforces one like per user per post
CREATE TABLE IF NOT EXISTS likes (
    post_id     INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (post_id, user_id)
);

-- ── Indexes ──────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_threads_author   ON threads(author_id);
CREATE INDEX IF NOT EXISTS idx_threads_created  ON threads(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_posts_thread     ON posts(thread_id, created_at);
CREATE INDEX IF NOT EXISTS idx_likes_post       ON likes(post_id);


-- ============================================================
-- Queries
-- ============================================================

-- ── Auth ─────────────────────────────────────────────────────────────────────

-- Register: insert user, return id
INSERT INTO users (username, email, password_hash)
VALUES (?, ?, ?)
RETURNING id;

-- Login: fetch user for credential check
SELECT id, username, password_hash
FROM   users
WHERE  username = ?
LIMIT  1;

-- ── Threads ──────────────────────────────────────────────────────────────────

-- List threads (supports optional full-text search via LIKE)
SELECT  t.id,
        t.title,
        t.author_id,
        u.username  AS author,
        (SELECT COUNT(*) FROM posts p WHERE p.thread_id = t.id) AS post_count,
        t.created_at,
        t.updated_at
FROM    threads t
JOIN    users   u ON u.id = t.author_id
WHERE   (:search IS NULL OR t.title LIKE '%' || :search || '%')
ORDER BY t.created_at DESC
LIMIT  :limit OFFSET :offset;

-- Get single thread
SELECT  t.id,
        t.title,
        t.author_id,
        u.username AS author,
        t.created_at,
        t.updated_at
FROM    threads t
JOIN    users   u ON u.id = t.author_id
WHERE   t.id = ?;

-- Create thread, return id
INSERT INTO threads (title, author_id)
VALUES (?, ?)
RETURNING id;

-- Update thread (author only)
UPDATE threads
SET    title      = ?,
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE  id = ? AND author_id = ?;

-- Delete thread (author only; cascades to posts + likes)
DELETE FROM threads
WHERE  id = ? AND author_id = ?;

-- ── Posts ─────────────────────────────────────────────────────────────────────

-- List posts in a thread (includes like count + viewer's own like flag)
SELECT  p.id,
        p.thread_id,
        p.author_id,
        u.username AS author,
        p.body,
        (SELECT COUNT(*)  FROM likes l WHERE l.post_id = p.id)               AS like_count,
        (SELECT COUNT(*)  FROM likes l WHERE l.post_id = p.id
                                         AND l.user_id = :viewer)            AS liked_by_me,
        p.created_at,
        p.updated_at
FROM    posts p
JOIN    users u ON u.id = p.author_id
WHERE   p.thread_id = :thread_id
ORDER BY p.created_at ASC
LIMIT  :limit OFFSET :offset;

-- Get single post
SELECT  p.id,
        p.thread_id,
        p.author_id,
        u.username AS author,
        p.body,
        p.created_at,
        p.updated_at
FROM    posts p
JOIN    users u ON u.id = p.author_id
WHERE   p.id = ?;

-- Create post, return id
INSERT INTO posts (thread_id, author_id, body)
VALUES (?, ?, ?)
RETURNING id;

-- Update post body (author only)
UPDATE posts
SET    body       = ?,
       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE  id = ? AND author_id = ?;

-- Delete post (author only; cascades to likes)
DELETE FROM posts
WHERE  id = ? AND author_id = ?;

-- ── Likes ─────────────────────────────────────────────────────────────────────

-- Add like (idempotent)
INSERT OR IGNORE INTO likes (post_id, user_id) VALUES (?, ?);

-- Remove like
DELETE FROM likes WHERE post_id = ? AND user_id = ?;

-- Like status for a viewer
SELECT  COUNT(*)                                                   AS count,
        SUM(CASE WHEN user_id = :viewer THEN 1 ELSE 0 END)        AS liked_by_me
FROM    likes
WHERE   post_id = ?;
