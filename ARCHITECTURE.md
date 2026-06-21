# Vyuh Architecture

Vyuh is an Axum-based Rust web framework for building typed JSON APIs. The core
architecture is handler-first: application code defines routes, commands,
signals, tasks, and OpenAPI metadata through typed functions and bundle
registration rather than through a separate configuration layer. The handlers can be registered using code as well as macros

## Workspace Layout

- `vyuh/` contains the runtime framework crate.
- `vyuh-macros/` contains procedural macros used by the runtime crate.
- `docs/` contains subsystem-level documentation, one independent markdown file
  per subsystem.
- `vyuh/examples/` contains runnable examples and comparisons.
- `migrations/` contains project-level migration examples.

## Runtime Crate

The `vyuh` crate is organized around these subsystems:

- `site` builds and serves a `Site`, wires bundles into Axum, initializes
  services, logging, emitters, commands, and database access.
- `conf` defines `SiteConf`, environment loading, and runtime configuration
  validation.
- `bundles` is the composition layer for routes, commands, signals, emitters,
  services, migrations, schema contributors, docs, and assets.
- `routes` defines route metadata, method handling, middleware helpers, and
  built-in route behavior.
- `callables` provides the type-erased invocation model used by routes,
  commands, signals, emitters, and tasks.
- `db` provides backend-selected SQLx aliases, query builders, sessions,
  placeholder handling, mock sessions, and database errors.
- `auth` and `roles` provide JWT authentication and bit-mask role support.
- `validation` and `validators` provide typed validation primitives and
  extractor integration.
- `signals`, `emitters`, `debounce`, and `beacon` provide in-process events,
  scheduled event sources, Postgres notification sources, and live channels.
- `tasks` provides typed background task registration and Postgres-backed task
  execution.
- `commands` provides typed command registration and command dispatch through a
  built `Site`.
- `apidocs` and `schema` generate OpenAPI and schema output from registered
  operations and types.
- `assets`, `templates`, and `embed` provide embedded assets, server-side
  templates, and private bundle resources.
- `logging` configures structured tracing output.

## Macro Crate

The `vyuh-macros` crate exposes derive and attribute macros that keep user code
compact while feeding metadata into the runtime:

- Route, command, signal, emitter, task, cron, periodic, and asset macros
  generate bundle parts.
- `Bindable`, `Scannable`, `Filterable`, `Validate`, and role/schema macros
  generate database, validation, schema, and auth integration code.
- Macro implementation should keep parsing, validation, diagnostics, and code
  generation separated.

## Backend Model

Exactly one database backend feature must be enabled:

- `postgres` is the default and primary supported backend.
- `mysql` and `sqlite` are intended for cross-database query surfaces.
- Postgres-only capabilities such as LISTEN/NOTIFY and the concrete task store
  must stay behind Postgres cfg boundaries.

Backend selection belongs in `vyuh/src/db/commons.rs`, where `Database`,
`Arguments`, `Row`, `QueryResult`, and `Pool` are aliased.

## Request Flow

1. A bundle registers routes, commands, emitters, services, schema contributors,
   templates, assets, signals, and optional migrations.
2. `build_site` validates `SiteConf` and bundle metadata.
3. `SiteBuilder` creates the database pool, router, template engine,
   authenticator, command registry, signal engine, emitter engine, services, and
   Postgres task engine when available.
4. Axum routes receive `Site` as state and handlers use typed extractors.
5. Handlers call query builders or services and return typed responses.
6. OpenAPI and schema metadata are produced from registered operations and
   type metadata.

## Extension Rules

- Prefer adding behavior through bundles and typed subsystem registries.
- Keep backend-specific behavior isolated behind backend cfgs.
- Keep `mod.rs` files as module wiring and re-export surfaces.
- Keep public APIs fallible and explicit.
- Add tests for non-trivial behavior at the subsystem boundary.
