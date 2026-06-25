# Forum API Reference

All protected endpoints require `Authorization: Bearer <access_token>`.
Owner-only endpoints return `403 Forbidden` if the caller is not the resource author.

---

## Auth

| Method | Path              | Body                              | Auth     | Response                          |
|--------|-------------------|-----------------------------------|----------|-----------------------------------|
| POST   | /auth/register    | `{username, email, password}`     | —        | `{id, username}`                  |
| POST   | /auth/login       | `{username, password}`            | —        | `{access_token, refresh_token}`   |

---

## Threads

| Method | Path              | Filters / Body                              | Auth     | Permission   |
|--------|-------------------|---------------------------------------------|----------|--------------|
| GET    | /threads          | `?search=&limit=50&offset=0`                | Optional | Public       |
| POST   | /threads          | `{title}`                                   | Required | Any user     |
| GET    | /threads/{id}     | —                                           | Optional | Public       |
| PUT    | /threads/{id}     | `{title}`                                   | Required | Owner only   |
| DELETE | /threads/{id}     | —                                           | Required | Owner only   |

**GET /threads filters**

| Param    | Type    | Default | Description                        |
|----------|---------|---------|------------------------------------|
| `search` | string  | —       | Case-insensitive title substring   |
| `limit`  | integer | 50      | Max results (1–100)                |
| `offset` | integer | 0       | Pagination offset                  |

**Thread response shape**

```json
{
  "id": 1,
  "title": "Hello world",
  "author": "alice",
  "author_id": 7,
  "post_count": 12,
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-01-02T12:00:00Z"
}
```

---

## Posts

| Method | Path                            | Filters / Body      | Auth     | Permission   |
|--------|---------------------------------|---------------------|----------|--------------|
| GET    | /threads/{thread_id}/posts      | `?limit=50&offset=0` | Optional | Public       |
| POST   | /threads/{thread_id}/posts      | `{body}`            | Required | Any user     |
| PUT    | /posts/{id}                     | `{body}`            | Required | Owner only   |
| DELETE | /posts/{id}                     | —                   | Required | Owner only   |

**GET /threads/{thread_id}/posts filters**

| Param    | Type    | Default | Description         |
|----------|---------|---------|---------------------|
| `limit`  | integer | 50      | Max results (1–100) |
| `offset` | integer | 0       | Pagination offset   |

**Post response shape**

```json
{
  "id": 42,
  "thread_id": 1,
  "author": "bob",
  "author_id": 9,
  "body": "Great post!",
  "like_count": 3,
  "liked_by_me": true,
  "created_at": "2025-01-03T08:00:00Z",
  "updated_at": "2025-01-03T08:00:00Z"
}
```

> `liked_by_me` is `false` for unauthenticated viewers.

---

## Likes

| Method | Path                  | Body | Auth     | Description                 |
|--------|-----------------------|------|----------|-----------------------------|
| POST   | /posts/{id}/like      | —    | Required | Add like (idempotent)       |
| DELETE | /posts/{id}/like      | —    | Required | Remove like                 |
| GET    | /posts/{id}/like      | —    | Optional | Get count + viewer status   |

**Like status response**

```json
{ "count": 5, "liked_by_me": true }
```

---

## Permissions summary

| Resource         | Read     | Create   | Update        | Delete        |
|------------------|----------|----------|---------------|---------------|
| Thread           | Public   | Any user | Owner only    | Owner only    |
| Post             | Public   | Any user | Owner only    | Owner only    |
| Like             | Public   | Any user | n/a           | Own like only |

---

## Error responses

All errors follow the shape `{"error": "<message>"}` with the appropriate HTTP status code.

| Code | Meaning                                           |
|------|---------------------------------------------------|
| 400  | Bad request / validation failure                  |
| 401  | Missing or invalid JWT                            |
| 403  | Authenticated but not permitted (not owner)       |
| 404  | Resource not found                                |
| 409  | Conflict (duplicate username / email)             |
| 500  | Internal server error                             |
