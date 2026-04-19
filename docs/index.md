# UXAR Feature Docs

Opinionated Rust web framework built on Axum for Postgres-backed JSON APIs with JWT auth.
Each feature doc lives at `docs/features/<feature-name>.md`.

| Feature | File | Description |
| ------- | ---- | ----------- |
| Site Lifecycle | [site-lifecycle.md](features/site-lifecycle.md) | Constructs, boots, and serves the application. Entry point for wiring all subsystems into a running `Site`. |
| Configuration | [configuration.md](features/configuration.md) | Declares runtime config via `SiteConf`. Secrets come from env vars; structure and logic stay in source code. |
| Routing | [routing.md](features/routing.md) | Maps HTTP methods and URL patterns to async handler functions. Handlers are composed into bundles before registration. |
| Extractors | [extractors.md](features/extractors.md) | Pulls typed data (path params, query, body, headers, state) out of incoming requests into handler arguments. |
| Layers | [layers.md](features/layers.md) | Tower middleware applied around the request/response pipeline. Controls path normalization, panic recovery, and custom interceptors. |
| Auth and Roles | [auth-and-roles.md](features/auth-and-roles.md) | JWT-based authentication with configurable access/refresh cookies and bit-mask role model for authorization checks. |
| Validation | [validation.md](features/validation.md) | Declarative field-level validation via the `Validate` trait and `Valid<T>` extractor. Errors are structured with field paths and codes. |
| Callables | [callables.md](features/callables.md) | Type-erased, introspectable function wrappers used as the internal dispatch primitive for signals, emitters, and tasks. |
| Tasks and Flows | [tasks-and-flows.md](features/tasks-and-flows.md) | Background unit tasks (async, run-once) and flow tasks (sync, can spawn children) backed by a Postgres task store. |
| Scheduling | [scheduling.md](features/scheduling.md) | Time-based execution via cron expressions (`#[cron]`) and fixed-interval polling (`#[periodic]`), registered per bundle. |
| Signals | [signals.md](features/signals.md) | Typed in-process event dispatch with optional debouncing. Handlers registered in bundles and triggered from anywhere in the app. |
| Emitters | [emitters.md](features/emitters.md) | Drives cron, periodic, and Postgres LISTEN/NOTIFY event sources. Delivers payloads to registered signal handlers. |
| Bundle Composition | [bundles.md](features/bundles.md) | Collects routes, tasks, signals, emitters, and services into a `Bundle` for incremental, composable registration with `Site`. |
| Schema and API Docs | [schema-and-apidoc.md](features/schema-and-apidoc.md) | Generates OpenAPI specs and serves interactive API documentation from route and type metadata collected at bundle registration. |
| Assets, Templates, Embed | [assets-templates-embed.md](features/assets-templates-embed.md) | Serves static files, renders server-side templates, and embeds asset directories into the binary at compile time. |
| Logging | [logging.md](features/logging.md) | Configures structured `tracing`-based logging with named rules, per-rule env-var overrides, file rotation, and level filtering. |
