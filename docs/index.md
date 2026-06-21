# Vyuh Docs

Vyuh documentation is organized by subsystem. Each subsystem page describes the
purpose, public APIs, runtime behavior, examples, and common failure modes for
that part of the framework.

## Subsystems

- [Site](site.md): application configuration, build/serve/test lifecycle,
  subsystem handles, routing access, and shutdown coordination.
- [Routes](routes.md): HTTP route registration, reverse routing, Bundle
  composition, and middleware metadata.
- [Middlewares](middlewares.md): site-wide HTTP transport policy, request IDs,
  panic catching, CORS, compression, limits, and slash behavior.
- [Request Data](request-data.md): Vyuh-owned `Json`, `Query`, `Path`, `Form`,
  and raw body wrappers.
- [Validation](validation.md): explicit `Valid<E>` request validation,
  validation errors, and OpenAPI constraint metadata.
- [Bundles](bundles.md): composition API for registering, merging, prefixing,
  validating, and documenting feature parts.
- [OpenAPI](openapi.md): generated OpenAPI specs, schema inference, response
  metadata, and explicit overrides.
- [Auth](auth.md): JWT configuration, token issuing, authenticated route
  extraction, role permits, and OpenAPI bearer security metadata.
- [Signals](signals.md): typed in-process events and signal handlers.
- [Emitters](emitters.md): scheduled and notification-driven event sources.
- [Database](db.md): SQLx-backed database access, query builders, derives,
  placeholders, and sessions.
- [Tasks](tasks.md): durable background tasks, continuation state, topic
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
