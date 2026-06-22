# Vyuh

[![Crates.io](https://img.shields.io/crates/v/vyuh)](https://crates.io/crates/vyuh)
[![docs.rs](https://img.shields.io/docsrs/vyuh)](https://docs.rs/vyuh)
[![License](https://img.shields.io/crates/l/vyuh)](LICENSE)

_Vyuh_ (व्यूह, _vyoo-huh_) means "formation", "arrangement", or "structured
configuration".

Vyuh is a handler-first Rust web framework built on Axum and SQLx.

It is designed around one idea: application structure should stay visible in
ordinary Rust code. Handler signatures describe what a route, command, task, or
signal consumes. Bundles group related application parts. `Site` owns the
runtime. `Data<T>` gives typed application data the same shape across
subsystems.

Vyuh favors a narrow, explicit API over hidden framework magic. Request wrappers
parse. `Valid<E>` validates. Auth is opt-in. Tasks retry only when the task says
so. Services stay separate because they are site-lifetime components, not
handler data.

Vyuh is usable, but not API-stable yet. Expect breaking changes before a stable
release.

## The Shape

Vyuh tries to make the common application path cohesive:

- `Data<T>` is typed application data.
- `Valid<E>` opts into validation.
- `Site` is the runtime handle and lifecycle surface.
- Bundles are the composition unit for features.
- Handler signatures drive extraction, auth, validation, OpenAPI, and subsystem
  access.
- Macros are convenience syntax. The same routes, commands, tasks, signals,
  emitters, services, assets, and OpenAPI wiring can be registered with the
  direct API.

The goal is not to hide the framework. The goal is to keep each framework
concept small, explicit, and in the same place as the code that needs it.

## Getting Started

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, Site, SiteConf, Valid, Validate, bundles};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Validate)]
struct Signup {
    #[validate(email)]
    email: String,

    #[validate(min_length = 3, max_length = 80)]
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct UserCreated {
    id: i64,
    email: String,
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct SystemPulse {
    project: String,
}

#[bundles::route(path = "/users", method = "POST")]
async fn signup(Valid(Data(input)): Valid<Data<Signup>>) -> Result<Data<UserCreated>, Error> {
    Ok(Data::new(UserCreated {
        id: 1,
        email: input.email.clone(),
        name: input.name.clone(),
    }))
}

#[bundles::cron(expr = "0 */5 * * * *")]
async fn heartbeat(site: Site) -> Data<SystemPulse> {
    Data::new(SystemPulse {
        project: site.project_dir().display().to_string(),
    })
}

#[bundles::signal]
async fn record_heartbeat(Data(pulse): Data<SystemPulse>) {
    println!("heartbeat for {}", pulse.project);
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let app = bundles::bundle! {
        signup,
        heartbeat,
        record_heartbeat,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Vyuh Example")
            .version("0.1.0")
            .description("Routes, typed data, emitters, signals, and OpenAPI.")
            .spec("/openapi.json"),
    )
    .with_prefix("/api");

    Site::run(SiteConf::from_env_with_files()?, app).await
}
```

This bundle registers a validated JSON route, a cron emitter, a signal handler,
and an OpenAPI spec endpoint. The route parses and validates `Data<Signup>`,
returns `Data<UserCreated>` as JSON, and contributes request, response, and
validation metadata to OpenAPI from the handler signature. Without `Valid`,
`Data<Signup>` would parse only.

`Site::run` is the normal application entrypoint. It builds the site, runs the
requested command, and defaults to serving HTTP when no command is supplied. Use
`Site::serve` for server-only binaries and `Site::build` when embedding or
testing needs the built site object.

## Bundles

A bundle is Vyuh's feature composition unit.

A feature can own its routes, templates, assets, services, tasks, commands,
signals, emitters, and OpenAPI metadata together. Larger applications are built
by merging and prefixing bundles instead of scattering feature wiring across
unrelated global registries.

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

## Philosophy

Vyuh is built around simplicity and uniformity:

- One typed data wrapper, `Data<T>`, is shared across routes, commands, tasks,
  signals, and emitters.
- Validation is explicit through `Valid<E>`, never automatic because a type
  derives `Validate`.
- Auth is completely opt-in; routes without auth extractors do no auth work.
- Errors have a common application shape, then render differently for HTTP,
  commands, and tasks.
- Site-level subsystems are accessed through direct handles such as `site.db()`,
  `site.tasks()`, `site.templates()`, `site.file_storage()`, and `site.auth()`.
- Services remain distinct because they represent site-lifetime components, not
  handler input or output.
- Console is opt-in and read-only in its first pass, for operational inspection
  without exposing command or task mutation APIs.

This keeps the framework cohesive without pretending every subsystem is the
same thing.

## Documentation

- [Site](docs/site.md): lifecycle, configuration, subsystem handles, and
  testing.
- [Bundles](docs/bundles.md): feature composition, prefixing, OpenAPI, and
  validation.
- [Routes](docs/routes.md): handler-first HTTP routes and route metadata.
- [Request](docs/request.md): `Data`, JSON, query, path, forms, multipart, and
  raw bodies.
- [Response](docs/response.md): response wrappers, redirects, headers, and
  metadata.
- [Validation](docs/validation.md): explicit `Valid<E>`, structured errors, and
  schema hints.
- [Errors](docs/errors.md): application errors, rendered errors, commands, and
  tasks.
- [Auth](docs/auth.md): opt-in JWT, API keys, static roles, dynamic
  permissions, and Django password hashes.
- [OpenAPI](docs/openapi.md): generated specs from handler signatures and
  overrides.
- [Middlewares](docs/middlewares.md): site-wide HTTP policy and slash behavior.
- [Database](docs/db.md): SQLx access, query helpers, sessions, and
  transactions.
- [Tasks](docs/tasks.md): durable single-unit background continuations.
- [Commands](docs/commands.md): site-aware CLI commands for operations.
- [Services](docs/services.md): site-lifetime components and workers.
- [Templates](docs/templates.md): Minijinja configuration, helpers, and
  formatting.
- [Assets](docs/assets.md): embedded assets, public files, and `collect_static`.
- [Uploads](docs/uploads.md): multipart uploads, MIME screening, and file
  storage.
- [Signals](docs/signals.md): typed in-process event handling.
- [Channels](docs/channels.md): live client-facing pub/sub over SSE,
  WebSocket, and long polling.
- [Emitters](docs/emitters.md): cron, periodic, debounced, and notification
  sources.
- [Logging](docs/logging.md): tracing setup, sinks, and runtime logging.
- [Console](docs/console.md): optional JSON APIs for operations, task records,
  and runtime status.

See [docs/index.md](docs/index.md) for the full documentation index.

## Backend Support

Vyuh is Postgres-first where database semantics matter most, but the common
database and task surfaces support Postgres, MySQL, and SQLite.

Postgres is the preferred backend for high-concurrency task workers and
notification emitters. MySQL is supported for SQLx access and task storage.
SQLite is useful for local, embedded, and single-process deployments.

## Current Caveats

- Vyuh is usable, but not API-stable yet. Expect breaking changes before a
  stable release.
- Services are in-process and not durable.
- Tasks provide durable single-task continuations, not multi-task workflow
  orchestration.
- SQLite task storage is not positioned as a high-concurrency production worker
  backend.
- Some Postgres features, such as `LISTEN`/`NOTIFY` and `RETURNING *` helpers,
  are intentionally Postgres-only.

## License

Vyuh is licensed under the [MIT License](LICENSE).
