# Vyuh Docs

Vyuh documentation is organized by subsystem. Each subsystem page describes the
purpose, public APIs, runtime behavior, examples, and common failure modes for
that part of the framework.

## Vocabulary

- `Data<T>`: handler input/output data for routes, commands, tasks, signals,
  emitters, and live channel delivery.
- `Valid<E>`: explicit validation wrapper around parsed request or handler data.
- `ServiceRef<T>`: site-lifetime service access. Services are not handler data.
- `Site`: cheap runtime handle for subsystem access.
- `SiteConfig`: extracted runtime configuration for handlers that need it.
- `Bundle`: feature composition unit for routes, commands, tasks, signals,
  emitters, services, assets, and OpenAPI.
- `ErrorView`: transport-neutral error renderer input.
- `ErrorReport`: default HTTP JSON error body.

## Choosing The Right Subsystem

| Need | Use |
| --- | --- |
| HTTP request handling | [Routes](routes.md) |
| Request parsing and body extraction | [Request](request.md) |
| Response wrappers and metadata | [Response](response.md) |
| Runtime validation | [Validation](validation.md) |
| One-shot site-aware CLI operations | [Commands](commands.md) |
| Durable retryable background work | [Tasks](tasks.md) |
| In-process fanout | [Signals](signals.md) |
| Scheduled or external event sources | [Emitters](emitters.md) |
| Client-facing live delivery | [Channels](channels.md) |
| Site-lifetime clients, caches, and workers | [Services](services.md) |
| Opt-in auth and verified principals | [Auth](auth.md) |
| SQLx-backed persistence | [Database](db.md) |
| Multipart uploads and runtime files | [Uploads](uploads.md) |
| Static public/private bundle files | [Assets](assets.md) |
| Server-side rendering | [Templates](templates.md) |
| Generated API specs | [OpenAPI](openapi.md) |
| Error normalization and rendering | [Errors](errors.md) |
| Optional operational inspection | [Console](console.md) |

## Common Combinations

- JSON API: [Routes](routes.md), [Request](request.md),
  [Response](response.md), [Validation](validation.md), [Errors](errors.md),
  and [OpenAPI](openapi.md).
- Admin CLI: [Commands](commands.md), [Site](site.md), and
  [Errors](errors.md).
- Durable async work: [Tasks](tasks.md), [Database](db.md), and
  [Commands](commands.md).
- Live UI updates: [Signals](signals.md), [Emitters](emitters.md), and
  [Channels](channels.md).
- Authenticated API: [Auth](auth.md), [Routes](routes.md),
  [Errors](errors.md), and [OpenAPI](openapi.md).

## Subsystems

- [Site](site.md): application configuration, build/serve/test lifecycle,
  subsystem handles, routing access, and shutdown coordination.
- [Routes](routes.md): HTTP route registration, reverse routing, Bundle
  composition, and middleware metadata.
- [Middlewares](middlewares.md): site-wide HTTP transport policy, request IDs,
  panic catching, CORS, compression, limits, and slash behavior.
- [Request](request.md): Vyuh-owned `Data`, `Json`, `Query`, `Path`, `Form`,
  multipart, and raw body wrappers.
- [Uploads](uploads.md): multipart forms, MIME sniffing, large upload handling,
  `LocalStorage`, and safe runtime file storage.
- [Response](response.md): response wrappers, redirects, headers, raw
  responses, and OpenAPI response metadata.
- [Validation](validation.md): explicit `Valid<E>` request validation,
  structured validation errors, custom schema hints, and OpenAPI constraint
  metadata.
- [Errors](errors.md): application errors, subsystem errors, HTTP
  `ErrorReport`, command rendering, and task retry semantics.
- [Bundles](bundles.md): composition API for registering, merging, prefixing,
  validating, and documenting feature parts.
- [OpenAPI](openapi.md): generated OpenAPI specs, schema inference, response
  metadata, and explicit overrides.
- [Auth](auth.md): opt-in JWT and API-key extraction, static roles, dynamic
  permissions, Django password hashes, and OpenAPI security metadata.
- [Signals](signals.md): typed in-process events and signal handlers.
- [Channels](channels.md): signal-backed live client delivery over SSE,
  WebSocket, and long polling with bounded replay.
- [Emitters](emitters.md): scheduled, debounced, and notification-driven event
  sources.
- [Database](db.md): SQLx-backed database access, query builders, derives,
  placeholders, and sessions.
- [Tasks](tasks.md): durable background tasks, continuation state, ID-based
  resume, and bounded concurrency.
- [Commands](commands.md): site-aware CLI commands for admin, diagnostics,
  maintenance, and one-off operations.
- [Assets](assets.md): bundle-owned public assets, private templates and
  resources, release embedding, debug filesystem reads, and `collect_static`.
- [Templates](templates.md): Minijinja-backed server-side rendering, environment
  options, helper filters/functions, and date/time formatting.
- [Services](services.md): site-lifetime application services, route
  injection, trait facades, and service-owned workers.
- [Logging](logging.md): structured tracing configuration and runtime logging.
- [Console](console.md): opt-in read-only JSON APIs for operational
  inspection, task records, and runtime status.
