# Configuration

## Purpose

- Defines runtime settings for networking, DB, auth, tasks, paths, timezone, and logging.
- Loads configuration from defaults, environment variables, and optional `.env` files.
- Validates required values and filesystem paths before site bootstrap.

## API Surface

- Name: `ConfError`
  Kind: `enum`
  Signature: `pub enum ConfError { RequiredField, InvalidValue, InvalidPath, Many, MissingField, Other }`
  Inputs: validation and env parsing failures.
  Output: unified configuration error.
  Errors: `None`.
  Side Effects: `None`.

- Name: `StaticDir`
  Kind: `struct`
  Signature: `pub struct StaticDir { pub path: String, pub url: String }`
  Inputs: filesystem path and URL mount prefix.
  Output: one static directory mount rule.
  Errors: validated by `SiteConf::validate()`.
  Side Effects: `None`.

- Name: `SiteConf`
  Kind: `struct`
  Signature: `pub struct SiteConf { host, port, project_dir, database, secret_key, static_dirs, media_dir, templates_dir, touch_reload, log_init, tz, auth, tasks, logging }`
  Inputs: explicit field values, defaults, builder methods, and env patches.
  Output: immutable site bootstrap config.
  Errors: `ConfError` through env parsing or validation.
  Side Effects: `None`.

- Name: config entrypoints
  Kind: `fn`
  Signature: `workspace_root`, `project_dir`, `SiteConf::default`, `with_env`, `from_env`, `from_env_with_files`, `load_env_files`, `load_env_file`, `validate`
  Inputs: optional env state or explicit file path.
  Output: resolved config values or validation result.
  Errors: `ConfError` from env parsing or validation.
  Side Effects: loads `.env` files into process env for the current process.

- Name: builder methods
  Kind: `fn`
  Signature: `host`, `port`, `project_dir`, `database`, `secret_key`, `static_dir`, `media_dir`, `templates_dir`, `touch_reload`, `log_init`, `timezone`, `auth`, `tasks`
  Inputs: replacement values for a `SiteConf` field.
  Output: updated `SiteConf`.
  Errors: `None` at call time.
  Side Effects: `None`.

## Usage Examples

### Example 1

Goal: Start from defaults and validate.

```rust
use uxar::SiteConf;

let conf = SiteConf::default().host("127.0.0.1").port(8080);
let _ = conf.validate();
```

Why valid:

- Builder methods consume and return `SiteConf`.
- Validation is explicit.

### Example 2

Goal: Load config from `.env` files and environment.

```rust
use uxar::SiteConf;

let conf = SiteConf::from_env_with_files()?;
# Ok::<(), uxar::ConfError>(())
```

Why valid:

- `.env` loading and env patching are exposed through one entrypoint.
- The returned error type is `ConfError`.

### Example 3

Goal: Configure static and template directories.

```rust
use uxar::SiteConf;

let conf = SiteConf::default()
    .project_dir(".")
    .static_dir("public", "/static")
    .templates_dir("templates");
```

Why valid:

- `static_dir` appends one `StaticDir` entry.
- Paths are interpreted relative to `project_dir` when relative.

## Behavior Rules

- MUST provide defaults for host, port, project dir, auth, tasks, and logging.
- MUST derive the default project dir from workspace root when `CARGO_MANIFEST_DIR` is available.
- MUST load `.env` first in `from_env_with_files()`.
- MUST load `.env.test` in test builds.
- MUST load `.env.dev` in debug non-test builds.
- MUST load `.env.prod` in non-debug non-test builds.
- MUST apply environment variables after `.env` files are loaded.
- MUST ignore unknown environment keys in `apply_env_patches`.
- MUST validate `secret_key`, `host`, `port`, DB settings, and configured paths.
- MUST reject an empty `secret_key`.
- MUST reject the default dev secret in non-debug builds.
- MUST reject `port == 0`.
- MUST reject `database.url == ""`.
- MUST reject `database.max_connections == 0` and `min_connections > max_connections`.
- MUST require every static URL to start with `/`.
- MUST resolve relative filesystem paths from `project_dir`.
- `log_init` is env-patchable and builder-settable, but current site bootstrap does not branch on it.

## Integration Guide

1. Start from `SiteConf::default()` or `SiteConf::from_env_with_files()`.
2. Override fields with builder methods for host, port, project paths, auth, tasks, or DB.
3. Add static, media, and template paths relative to the chosen `project_dir`.
4. Call `validate()` if code needs config-only failure before bootstrap.
5. Pass the finished config into `build_site`, `serve_site`, or `test_site`.
6. Keep secrets and deployment-specific values in env vars, not in source.

## Failure Modes

| Condition                                         | Observed Outcome           | Fix                                                 |
| ------------------------------------------------- | -------------------------- | --------------------------------------------------- |
| `secret_key` is empty                             | `ConfError::RequiredField` | Provide a non-empty secret key.                     |
| Release build uses the default dev secret         | `ConfError::InvalidValue`  | Set a custom secret through env or builder methods. |
| `port` is `0`                                     | `ConfError::InvalidValue`  | Use a port in the `1..=65535` range.                |
| `database.url` is empty                           | `ConfError::RequiredField` | Supply a valid database URL.                        |
| Relative or absolute directory path is unreadable | `ConfError::InvalidPath`   | Fix the path or permissions.                        |
| Static URL does not start with `/`                | `ConfError::InvalidValue`  | Use a leading slash in each static mount URL.       |
| `LOG_INIT` env value is not `true` or `false`     | `ConfError::Other`         | Use a valid boolean string.                         |

## Non-Goals

- Does not open sockets or start the site.
- Does not initialize tracing by itself.
- Does not document nested auth, task, or logging config in full detail.

## LLM Recipe

1. Generate `SiteConf` first.
2. Prefer `from_env_with_files()` for app entrypoints and `default()` plus builders for tests.
3. Set `project_dir` before adding relative paths.
4. Add static and template paths only when the feature needs them.
5. Keep secrets in env-backed configuration, not string literals, unless the example is explicitly local-only.
6. Call `validate()` in generated setup code when early config failure is useful.
7. Use env keys that current code understands: `DATABASE_URL`, `SECRET_KEY`, `HOST`, `PORT`, `TZ`, `LOG_INIT`.
8. Anti-pattern: claiming unknown env keys will error; current code ignores them.
9. Anti-pattern: claiming `log_init` disables logging in current bootstrap code.
10. Final check: ensure all referenced relative paths are anchored to `project_dir`.
