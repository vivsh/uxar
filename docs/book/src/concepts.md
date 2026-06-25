# Concepts

Vyuh is organized around a few small concepts.

## Site

`Site` is the built application. It owns configuration, database access,
templates, services, tasks, signals, emitters, commands, logging, and shutdown
coordination.

## Data

`Data<T>` is typed application data. Routes, commands, tasks, signals, and
emitters use it as the common data wrapper. Use `Valid<Data<T>>` when the
boundary should validate the parsed value.

## Bundles

Bundles are the composition unit. A feature can own its routes, services, tasks,
commands, signals, emitters, templates, and assets together.

## Services

Services are site-lifetime components. They are constructed when the site is
built and are accessed with `ServiceRef<T>` or `site.service::<T>()`.

## Subsystems

- [Routes](routes.md): handle HTTP requests with typed extractors, responses,
  validation, and route metadata.
- [Request Data](request.md): parse path, query, form, JSON, multipart, and raw
  body input into Vyuh wrappers.
- [Responses](response.md): return typed responses, redirects, headers, raw
  bodies, and documented response shapes.
- [Validation](validation.md): make boundary validation explicit with `Valid<E>`
  and structured errors.
- [Errors](errors.md): normalize application, subsystem, HTTP, command, and task
  failures.
- [Auth](auth.md): add opt-in principals, roles, permissions, and OpenAPI
  security metadata.
- [Middlewares](middlewares.md): configure site-wide HTTP behavior such as CORS,
  compression, limits, request IDs, and slash policy.
- [OpenAPI](openapi.md): generate API specifications from route, request,
  response, validation, and auth metadata.
- [Tasks](tasks.md): run durable retryable background work that can survive
  process restarts.
- [Commands](commands.md): run site-aware CLI operations for administration,
  diagnostics, and maintenance.
- [Signals](signals.md): fan out typed in-process events to registered handlers.
- [Emitters](emitters.md): produce scheduled, debounced, or externally triggered
  events.
- [Channels](channels.md): deliver live client-facing messages over SSE,
  WebSocket, or long polling.
- [Templates](templates.md): render server-side HTML with Minijinja and
  framework helpers.
- [Assets](assets.md): serve and collect bundle-owned public assets while
  keeping private resources internal.
- [Uploads](uploads.md): accept multipart files and store runtime uploads safely.
- [Database](db.md): access SQLx-backed persistence, sessions, query helpers,
  and backend-specific storage.
- [Logging](logging.md): configure structured tracing and runtime log output.
- [Console](console.md): inspect routes, tasks, services, commands, and runtime
  status through the opt-in console.
