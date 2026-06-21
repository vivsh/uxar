# Vyuh

Vyuh is a handler-first Rust web framework built on Axum and SQLx.

It is for applications where the handler signature should carry real weight:
routes, OpenAPI, auth requirements, typed inputs, background work, and subsystem
access all stay close to the code that uses them.

Vyuh is usable, but not API-stable yet. Expect breaking changes before a stable
release.

## The Idea

Most web frameworks make you describe the same endpoint several times: once in
the route, once in the handler, once in the OpenAPI spec, once in middleware,
and again in whatever background or service wiring the endpoint needs.

Vyuh tries to collapse that duplication.

```rust
use serde::Serialize;
use vyuh::{auth::{BitRole, permit}, bundles, routes::Json};

#[derive(BitRole)]
enum AppRole {
    Manager,
}

#[derive(Serialize)]
struct ProtectedStatus {
    ok: bool,
}

#[bundles::route(path = "/protected/status", method = "GET")]
async fn protected_status(_auth: permit!(AppRole, Manager)) -> Json<ProtectedStatus> {
    Json(ProtectedStatus { ok: true })
}
```

The handler is the route contract. Its arguments drive extraction, optional
validation, auth, and OpenAPI metadata. A role-protected endpoint looks
role-protected in the signature. A typed JSON response is reflected in the
spec. The framework does not need a second annotation language for the common
case.

## Quick Start

```rust
use serde::Serialize;
use vyuh::{SiteConf, bundles, routes::Json};

#[derive(Serialize)]
struct Hello {
    message: &'static str,
}

#[bundles::route(path = "/")]
async fn index() -> Json<Hello> {
    Json(Hello { message: "hello from vyuh" })
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        index,
    };

    vyuh::serve_site(SiteConf::from_env_with_files()?, bundle).await
}
```

Vyuh applications are built from bundles. A bundle can include routes, OpenAPI
specs, auth-protected handlers, services, durable tasks, signal handlers,
emitters, templates, assets, and commands. `SiteConf` describes the runtime;
`serve_site` builds the site and starts the server.

For tests or embedding, build without serving:

```rust
let site = vyuh::build_site(conf, bundle).await?;
let app = vyuh::testing::router(&site);
```

## What Vyuh Gives You

**Handler-first routes and OpenAPI**

Routes are ordinary async Rust functions. Handler signatures and doc-comments
produce OpenAPI metadata, including request bodies, responses, auth, and role
requirements. Vyuh request wrappers parse by default; wrap them in `Valid<E>`
when the route should enforce `#[validate(...)]` constraints and publish those
constraints in OpenAPI.

```rust
use vyuh::{Validate, routes::{Json, Valid}};

#[derive(serde::Deserialize, schemars::JsonSchema, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
}

async fn create(Valid(Json(input)): Valid<Json<CreateUser>>) {
    // parsed and validated
}
```

**Ergonomic auth**

JWT auth can be read from bearer headers or cookies. `AuthUser` extracts the
authenticated subject, and `permit!(Role, Manager | Editor)` protects a route
without separate middleware wiring.

**Consistent route errors**

Parse failures, validation errors, auth failures, database errors, template
errors, and application errors normalize into `ErrorReport`. Applications can
install an async site-wide error handler to render JSON, HTML, or any custom
response shape.

**Site-wide HTTP policy**

`SiteConf::http(...)` configures global transport behavior such as request IDs,
panic catching, tracing, compression, CORS, timeouts, body limits, security
headers, and deterministic trailing-slash policy. API routes and HTML pages can
use different slash behavior without hard-coded server normalization.

**A real application lifecycle**

`Site` owns the router, database pool, templates, services, tasks, logging,
signals, emitters, auth, commands, and shutdown coordination. Subsystems are
accessed through focused handles such as `site.tasks()`, `site.templates()`,
`site.db()`, and `site.service::<T>()`.

**Typed services**

Services are site-lifetime components for shared clients, caches, coordinators,
and in-process state. They can be injected into routes, expose trait facades,
and run service-owned background loops.

**Durable tasks**

Tasks are persisted single-unit continuations. A task can sleep, suspend on a
topic, resume with input, retry, maintain state, and survive process restarts.
They are not a workflow engine; they are durable state machines for one unit of
work.

```rust
site.tasks().submit(EmailJob {
    to: "user@example.com".into(),
}).await?;
```

**Templates and assets**

Minijinja templates can come from a project template directory or bundle asset
dirs. `SiteConf::templates(...)` configures the Minijinja environment,
autoescape, strict undefined behavior, whitespace settings, and date/time
formats. Templates get framework helpers such as `asset`, `url_for`,
`format_datetime`, and `localdate`. Public bundle assets live under `public/**`,
are served from `/assets`, read from the filesystem in debug builds, embedded in
release builds, and can be copied with `collect_static`.

**SQLx-first database access**

Vyuh keeps direct SQLx access available while adding small SQL-shaped builders,
typed bind/scanning derives, named placeholders, sessions, and transactions.

**Signals and emitters**

Signals provide typed in-process decoupling. Emitters can produce typed payloads
from cron schedules, periodic timers, or Postgres `LISTEN`/`NOTIFY`.

**Structured logging**

Logging is configured from `SiteConf` and uses standard `tracing` macros in
application code. Rules can target stdout, stderr, or rotating files, with
environment overrides.

## A Larger Shape

A feature can own everything it needs:

```rust
let dashboard = bundles::bundle! {
    dashboard_routes,
    dashboard_service,
    rebuild_dashboard_index,
    dashboard_assets,
};

let app = bundles::bundle! {
    public_routes,
    auth_routes,
    dashboard.with_prefix("/dashboard"),
};
```

The bundle is the composition unit. This keeps a feature from scattering its
routes, templates, static files, services, tasks, and OpenAPI metadata across
unrelated global registries.

## Backend Support

Vyuh is Postgres-first where database semantics matter most, but the common
database and task surfaces support more than Postgres.

| Area | Postgres | MySQL | SQLite |
| --- | --- | --- | --- |
| SQLx pool/session access | yes | yes | yes |
| Query builders | yes | yes | yes |
| Typed bind/scanning derives | yes | yes | yes |
| Durable task store | yes | yes | yes |
| High-concurrency task workers | preferred | supported on InnoDB with row locking | local/single-process only |
| `LISTEN`/`NOTIFY` emitters | yes | no | no |
| `RETURNING *` helpers | yes | no | no |

SQLite task storage is durable and useful for local, embedded, and
single-process deployments. For high-concurrency multi-worker task processing,
prefer Postgres or MySQL.

## Documentation

- [Site](docs/site.md): configuration, build/serve/test lifecycle, subsystem
  handles, routing access, and shutdown coordination.
- [Routes](docs/routes.md): route registration, reverse routing, bundle
  composition, and middleware metadata.
- [Middlewares](docs/middlewares.md): site-wide request IDs, panic catching,
  CORS, compression, limits, security headers, and slash behavior.
- [Request Data](docs/request-data.md): Vyuh-owned `Json`, `Query`, `Path`,
  `Form`, and raw body wrappers.
- [Validation](docs/validation.md): explicit `Valid<E>` request validation,
  validation errors, and OpenAPI constraint metadata.
- [Bundles](docs/bundles.md): the composition API for registering, merging,
  prefixing, validating, and documenting feature parts.
- [OpenAPI](docs/openapi.md): generated OpenAPI specs, schema inference,
  response metadata, and explicit overrides.
- [Auth](docs/auth.md): JWT configuration, token issuing, authenticated route
  extraction, `permit!`, roles, and OpenAPI bearer security metadata.
- [Database](docs/db.md): SQLx pools, query builders, bind/scanning derives,
  sessions, transactions, and backend boundaries.
- [Tasks](docs/tasks.md): durable coroutine-like background tasks with state,
  sleep, suspend/resume, retries, leases, and backend stores.
- [Commands](docs/commands.md): site-aware CLI commands for admin,
  diagnostics, maintenance, and one-off operations.
- [Services](docs/services.md): site-lifetime application services, route
  injection, trait facades, and service-owned workers.
- [Templates](docs/templates.md): Minijinja-backed rendering, environment
  options, helper filters/functions, and date/time formatting.
- [Assets](docs/assets.md): public assets, private resources, debug filesystem
  reads, release embedding, and `collect_static`.
- [Signals](docs/signals.md): typed in-process event handlers.
- [Emitters](docs/emitters.md): cron, periodic, and notification sources.
- [Logging](docs/logging.md): tracing configuration, log levels, sinks, and
  runtime logging behavior.

See [docs/index.md](docs/index.md) for the subsystem index.

## Examples

Examples live in [`vyuh/examples`](vyuh/examples).

- [`routes_basic.rs`](vyuh/examples/routes_basic.rs): minimal route registration.
- [`routes_direct.rs`](vyuh/examples/routes_direct.rs): direct route API.
- [`routes_reverse.rs`](vyuh/examples/routes_reverse.rs): named routes and
  reverse routing.
- [`routes_validation.rs`](vyuh/examples/routes_validation.rs): parse-only and
  validated request wrappers.
- [`middlewares_basic.rs`](vyuh/examples/middlewares_basic.rs): site-wide HTTP
  middleware configuration.
- [`middlewares_slashes.rs`](vyuh/examples/middlewares_slashes.rs): slash
  policy configuration.
- [`openapi_basic.rs`](vyuh/examples/openapi_basic.rs): basic OpenAPI setup.
- [`openapi_responses.rs`](vyuh/examples/openapi_responses.rs): response
  metadata overrides.
- [`auth_basic.rs`](vyuh/examples/auth_basic.rs): JWT token pair creation and
  `AuthUser` extraction.
- [`auth_roles_openapi.rs`](vyuh/examples/auth_roles_openapi.rs): role masks,
  `permit!`, and OpenAPI security metadata.
- [`db_basic.rs`](vyuh/examples/db_basic.rs): typed row scanning and filters.
- [`db_writes.rs`](vyuh/examples/db_writes.rs): inserts and updates with
  `Bindable`.
- [`db_raw_statement.rs`](vyuh/examples/db_raw_statement.rs): direct
  `Statement` execution.
- [`db_sqlx.rs`](vyuh/examples/db_sqlx.rs): direct SQLx use through `DbPool`.
- [`db_transactions.rs`](vyuh/examples/db_transactions.rs): transaction use.
- [`tasks_basic.rs`](vyuh/examples/tasks_basic.rs): macro task registration and
  typed submit.
- [`tasks_direct.rs`](vyuh/examples/tasks_direct.rs): direct task registration.
- [`tasks_sleep.rs`](vyuh/examples/tasks_sleep.rs): timed continuation state.
- [`tasks_suspend_resume.rs`](vyuh/examples/tasks_suspend_resume.rs):
  topic-based suspension and resume.
- [`tasks_concurrency.rs`](vyuh/examples/tasks_concurrency.rs): worker
  concurrency configuration.
- [`tasks_sqlite.rs`](vyuh/examples/tasks_sqlite.rs): SQLite-backed local task
  storage.
- [`tasks_mysql.rs`](vyuh/examples/tasks_mysql.rs): MySQL-backed task storage.
- [`commands_basic.rs`](vyuh/examples/commands_basic.rs): typed command args
  and direct command registration.
- [`commands_site.rs`](vyuh/examples/commands_site.rs): command extraction of
  `Site` and site configuration access.
- [`commands_reindex.rs`](vyuh/examples/commands_reindex.rs): operational
  command that rebuilds an in-process search index service.
- [`templates_basic.rs`](vyuh/examples/templates_basic.rs): project template
  directory configuration.
- [`templates_assets.rs`](vyuh/examples/templates_assets.rs): templates from a
  bundle asset dir.
- [`templates_route.rs`](vyuh/examples/templates_route.rs): `Templates` route
  extraction.
- [`templates_config.rs`](vyuh/examples/templates_config.rs): Minijinja
  environment configuration.
- [`templates_datetime.rs`](vyuh/examples/templates_datetime.rs): date/time
  formatting configuration and Rust utilities.
- [`services_basic.rs`](vyuh/examples/services_basic.rs): concrete service
  registration and route extraction.
- [`services_direct.rs`](vyuh/examples/services_direct.rs): direct service
  registration.
- [`services_facade.rs`](vyuh/examples/services_facade.rs): trait facade
  exposure.
- [`services_worker.rs`](vyuh/examples/services_worker.rs): service-owned
  background worker.
- [`signals_basic.rs`](vyuh/examples/signals_basic.rs): typed signal handlers.
- [`signals_direct.rs`](vyuh/examples/signals_direct.rs): direct signal
  registration.
- [`signals_multiple_handlers.rs`](vyuh/examples/signals_multiple_handlers.rs):
  multiple handlers for one payload type.
- [`signals_scheduled.rs`](vyuh/examples/signals_scheduled.rs): delayed
  in-process signals.
- [`emitters_periodic.rs`](vyuh/examples/emitters_periodic.rs): periodic
  emitters.
- [`emitters_direct.rs`](vyuh/examples/emitters_direct.rs): direct emitter API.
- [`emitters_cron.rs`](vyuh/examples/emitters_cron.rs): cron emitters.
- [`emitters_pgnotify.rs`](vyuh/examples/emitters_pgnotify.rs): Postgres
  notification emitters.
- [`logging_basic.rs`](vyuh/examples/logging_basic.rs): stdout and rotating
  file logging.

## Verification

The current workspace has been checked with:

```text
cargo check --workspace --all-targets
cargo test --workspace
cargo build --workspace --all-targets
cargo check -p vyuh --no-default-features --features sqlite --all-targets
cargo check -p vyuh --no-default-features --features mysql --all-targets
cargo check -p vyuh --no-default-features --features postgres --all-targets
```

The MySQL feature check may print Cargo's future-incompatibility note for the
transitive `num-bigint-dig` dependency through `sqlx-mysql`; that is not a Vyuh
code warning.

## Launch Caveats

- Vyuh is experimental and APIs may still change.
- JWT signing is HS256-only in v0.
- Services are in-process and not durable.
- Tasks provide durable single-task continuations, not multi-task workflow
  orchestration.
- SQLite task storage is not positioned as a high-concurrency production worker
  backend.
- Some Postgres features, such as `LISTEN`/`NOTIFY` and `RETURNING *` helpers,
  are intentionally Postgres-only.
