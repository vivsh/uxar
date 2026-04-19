# Site Lifecycle

## Purpose

- Builds a running `Site` from `SiteConf` and a `Bundle`.
- Starts task, emitter, and service engines before serving HTTP traffic.
- Exposes runtime access to config-derived services such as DB, auth, templates, timezone, and routing.

## API Surface

- Name: `Site`
  Kind: `struct`
  Signature: `pub struct Site { inner: Arc<SiteInner> }`
  Inputs: created by `build_site` or `test_site`.
  Output: cloneable runtime application handle.
  Errors: `None`.
  Side Effects: owns engine handles, shutdown notifier, and logging guard.

- Name: `SiteError`
  Kind: `enum`
  Signature: `pub enum SiteError { DatabaseError, ConfError, AssetError, TemplateFileError, AddressResolutionError, TimezoneError, FileWatchError, BundleError, TemplateError, ServeError, IOError, EmitterError, SignalError, LoggingError, ServiceError, ServiceNotFound }`
  Inputs: startup, serving, template, logging, emitter, and service failures.
  Output: unified site bootstrap and runtime error.
  Errors: `None`.
  Side Effects: `None`.

- Name: `build_site`
  Kind: `fn`
  Signature: `pub async fn build_site(conf: SiteConf, bundle: impl IntoBundle) -> Result<Site, SiteError>`
  Inputs: validated `SiteConf` and any `IntoBundle` value.
  Output: ready `Site` with engines started.
  Errors: `SiteError`.
  Side Effects: validates config and bundle, initializes logging, DB, templates, services, tasks, emitters.

- Name: `serve_site`
  Kind: `fn`
  Signature: `pub async fn serve_site(conf: SiteConf, bundle: impl IntoBundle) -> Result<(), SiteError>`
  Inputs: validated `SiteConf` and any `IntoBundle` value.
  Output: serves HTTP requests until shutdown.
  Errors: `SiteError`.
  Side Effects: binds TCP listener and runs Axum server with graceful shutdown.

- Name: `test_site`
  Kind: `fn`
  Signature: `pub async fn test_site(conf: SiteConf, bundle: impl IntoBundle, pool: Pool) -> Result<Site, SiteError>`
  Inputs: `SiteConf`, `IntoBundle`, and prebuilt `sqlx::Pool`.
  Output: ready `Site` using caller-supplied pool.
  Errors: `SiteError`.
  Side Effects: skips pool creation from config; starts engines.

- Name: `Site` runtime methods
  Kind: `fn`
  Signature: `uptime`, `iter_operations`, `shutdown_notifier`, `shutdown`, `shutdown_and_wait`, `project_dir`, `reverse`, `render_template`, `authenticator`, `tz`, `db`, `router`, `beacon`, `submit_typed_task`, `submit_task`, `submit_task_data`
  Inputs: receiver `&self` plus method-specific args.
  Output: runtime state, reverse routes, rendered templates, cloned DB/beacon, or submitted task IDs.
  Errors: `TemplateError`, `TaskError`, or `None` depending on method.
  Side Effects: shutdown notification, task submission, and router state cloning.

- Name: `Site` extractor support
  Kind: `trait`
  Signature: `impl FromRequestParts<Site> for Site` and `impl FromRef<Site> for beacon::Beacon`
  Inputs: Axum request parts and application state.
  Output: cloned `Site` or `Beacon` inside handlers.
  Errors: `StatusCode` for extractor rejection.
  Side Effects: `None`.

## Usage Examples

### Example 1

Goal: Build a site without starting the HTTP server.

```rust
use uxar::{SiteConf, build_site};
use uxar::bundles::Bundle;

async fn boot() -> Result<(), uxar::SiteError> {
    let conf = SiteConf::from_env_with_files()?;
    let site = build_site(conf, Bundle::new()).await?;
    site.shutdown_and_wait().await;
    Ok(())
}
```

Why valid:

- `build_site` accepts any `IntoBundle`, including `Bundle::new()`.
- Config loading is explicit before bootstrap.

### Example 2

Goal: Extract `Site` inside a handler.

```rust
use uxar::Site;

async fn health(site: Site) -> String {
    format!("uptime={}s", site.uptime().as_secs())
}
```

Why valid:

- `Site` implements Axum request-part extraction.
- Cloning `Site` is cheap because it wraps `Arc` state.

### Example 3

Goal: Submit a background task from request code.

```rust
use serde::Serialize;
use uxar::Site;

#[derive(Serialize)]
struct EmailJob {
    to: String,
}

async fn enqueue(site: Site) -> Result<String, uxar::tasks::TaskError> {
    let id = site
        .submit_task("send_email", EmailJob { to: "a@example.com".into() })
        .await?;
    Ok(id.to_string())
}
```

Why valid:

- `submit_task` accepts any `Serialize` payload.
- Task submission is a site-owned runtime capability.

## Behavior Rules

- MUST call `SiteConf::validate()` before site construction completes.
- MUST call `bundle.validate()` before route or engine startup completes.
- MUST parse `SiteConf.tz` into `chrono_tz::Tz` or fail site build.
- MUST default timezone to `UTC` when `SiteConf.tz` is `None`.
- MUST create or reuse the database pool before loading services.
- MUST inject templates before returning the built site.
- MUST initialize tracing during build and keep the returned `LoggingGuard` alive in `Site`.
- MUST start task runner, emitter engine, and service workers before `build_site` returns.
- MUST bind the HTTP listener only in `serve_site`.
- MUST use graceful shutdown in `serve_site` via the shutdown notifier and watch signal.
- MUST allow handlers to extract `Site` directly from Axum state.
- MUST NOT support route or bundle hot reload after site construction.
- SHOULD use `test_site` when tests need a caller-controlled DB pool.
- `SiteConf.log_init` exists in config but current `build_site` code does not branch on it.

## Integration Guide

1. Build `SiteConf` with builder methods or `from_env_with_files()`.
2. Construct a `Bundle` or any other `IntoBundle` value containing routes and other parts.
3. Call `build_site` for tests or embedding, or call `serve_site` for the HTTP entrypoint.
4. Extract `Site` inside handlers when code needs DB, auth, templates, reverse routing, or task submission.
5. Use `site.router()` only for testing or advanced router wrapping.
6. Call `shutdown()` or `shutdown_and_wait()` during controlled teardown.

## Failure Modes

| Condition                                       | Observed Outcome                                             | Fix                                                                      |
| ----------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------ |
| Invalid timezone string in `SiteConf.tz`        | `SiteError::TimezoneError` during build                      | Use a valid IANA timezone string or leave it unset.                      |
| Bundle contains accumulated registration errors | `SiteError::BundleError` during build                        | Fix duplicate names or invalid bundle parts before calling `build_site`. |
| Database config is invalid or connection fails  | `SiteError::ConfError` or `SiteError::DatabaseError`         | Correct DB settings and ensure the database is reachable.                |
| TCP bind fails in `serve_site`                  | `SiteError::IOError` or `SiteError::AddressResolutionError`  | Fix host or port and ensure the address is available.                    |
| Template injection fails                        | `SiteError::TemplateError` or `SiteError::TemplateFileError` | Fix the template directory path or embedded templates.                   |
| Logging init fails                              | `SiteError::LoggingError`                                    | Fix logging config, env filters, or log directory permissions.           |

## Non-Goals

- Does not define route, task, signal, or service metadata by itself.
- Does not provide runtime hot reload of routes or bundles.
- Does not replace feature-specific docs for bundles, logging, tasks, or configuration.

## LLM Recipe

1. Start from `SiteConf`, not from ad hoc globals.
2. Build or import a `Bundle` that contains all required routes and background parts.
3. Use `build_site` when code needs a ready runtime without opening a socket.
4. Use `serve_site` when code is the process entrypoint.
5. Extract `Site` in handlers only when runtime access is required.
6. Prefer `site.submit_task` for background work instead of spawning detached app logic manually.
7. Call `site.reverse` for named URLs instead of hardcoding duplicated paths.
8. Validate that timezone, templates, logging, and DB config are present before final output.
9. Anti-pattern: generating code that mutates route registration after build.
10. Anti-pattern: documenting `log_init` as a working startup switch in current code.
