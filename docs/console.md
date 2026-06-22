# Console

Vyuh console is an opt-in JSON API for operational inspection. It is disabled
by default, isolated from application auth, and read-only in this pass.

Use it for inspecting registered operations, task records, and runtime status.
Do not use it as an application admin framework or a command/task execution
surface.

## Mental Model

- Console is a built-in operational app mounted only when `ConsoleConf.enabled`
  is true.
- Console roles are separate from application roles.
- Console auth uses a console-only cookie/session. Normal app JWTs do not grant
  console access.
- The first pass exposes JSON APIs only; frontend UI can be added later.

## Configuration

Configuration lives on `SiteConf`:

```rust
use vyuh::{SiteConf, console::ConsoleConf};

let conf = SiteConf::default().console(
    ConsoleConf::default()
        .enabled(true)
        .path("/_console"),
);
```

Defaults:

| Field | Default |
| --- | --- |
| `enabled` | `false` |
| `path` | `/_console` |
| `bootstrap_token_ttl_seconds` | `300` |
| `print_bootstrap_url` | `LocalOnly` |
| `cookie_name` | `vyuh_console` |
| `page_size_default` | `50` |
| `page_size_max` | `250` |
| `status_cache_ttl_seconds` | `5` |

With `LocalOnly`, Vyuh prints a short-lived bootstrap URL only when the
configured host is `localhost`, `127.0.0.1`, or `::1`:

```text
Vyuh console enabled:
http://localhost:8080/_console/login?token=...
Token expires in 300 seconds.
```

Bootstrap tokens are in-memory and are not persisted.

## Roles

Console roles live under `vyuh::console`:

```rust
use vyuh::console::ConsoleRole;
```

The roles are `Viewer`, `Operator`, and `Admin`. In this read-only pass,
`Viewer` can access all console APIs. `Operator` and `Admin` are reserved for
future guarded operations.

These roles do not affect `AuthUser`, `permit!(...)`, API keys, or application
authorization.

## Endpoints

All endpoints are mounted under `ConsoleConf.path`.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/login?token=...` | consume bootstrap token and set console cookie |
| `POST` | `/api/logout` | clear console cookie |
| `GET` | `/api/session` | inspect current console session |
| `GET` | `/api/operations` | list/search operation metadata |
| `GET` | `/api/operations/{id}` | inspect one operation |
| `GET` | `/api/tasks` | list task records |
| `GET` | `/api/tasks/{id}` | inspect one task record |
| `GET` | `/api/status` | combined site, process, and system status |

There are no mutating endpoints in v1. Console cannot run commands, retry or
cancel tasks, fire signals, or control services.

## Operations

`/api/operations` is the single operation listing endpoint. Use query
parameters for filtering:

```text
/_console/api/operations?kind=route&q=user&hidden=false&limit=50
```

Supported filters:

- `kind`: `route`, `command`, `task`, `service`, `signal`, `cron`,
  `periodic`, `pgnotify`, or `api_doc`.
- `q`: text search across name, summary, description, and path.
- `tag`: operation tag.
- `owner`: operation owner.
- `hidden`: `true` or `false`.
- `limit` and `cursor`: offset-style pagination.

The response includes operation metadata derived from the same bundle operation
model used by routes, OpenAPI, commands, tasks, signals, emitters, and services.

## Tasks

`/api/tasks` lists task records without claiming or modifying them:

```text
/_console/api/tasks?status=pending&priority_min=10&limit=50
```

Supported filters:

- `status`: `pending`, `running`, `suspended`, `succeeded`, or `failed`.
- `name`: registered task name.
- `priority_min`: minimum task priority.
- `identity`: task identity.
- `q`: text search across name, identity, and last error.
- `limit` and `cursor`: offset-style pagination.

`/api/tasks/{id}` returns the safe task detail shape, including status,
attempts, priority, timing, identity, last error, and JSON payload/state/result
fields when they parse as JSON.

## Status

`/api/status` returns one redaction-safe object with:

- site fields: Vyuh version, package name, host, port, project directory,
  timezone, database backend, uptime, enabled compile-time features, operation
  count, command count, and service count;
- process fields: PID, executable path, current directory, argv, memory, virtual
  memory, CPU usage, and platform-supported thread/open-file counts;
- system fields: hostname, OS, kernel, architecture, CPU, load average, memory,
  swap, and boot time.

Console never exposes env vars, secrets, JWT keys, API keys, cookies, full
database URLs, or raw configuration.

Status is cached in-process for `ConsoleConf.status_cache_ttl_seconds`, default
5 seconds. Requests inside that window return the previous snapshot instead of
refreshing system/process information again.

## Current Limitations

- Console is read-only.
- Console has no bundled frontend yet.
- Console sessions are in-memory and process-local.
- Pagination uses offset cursors in this pass.
- Task listing is inspection-only and does not affect task leasing or retries.
