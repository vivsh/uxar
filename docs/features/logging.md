# Logging

## Purpose

- Defines tracing sinks and filters for stdout, stderr, or rotating files.
- Resolves rule filters from env variables with deterministic precedence.
- Integrates with site bootstrap through tracing initialization.

## API Surface

- Name: `LoggingError`
  Kind: `enum`
  Signature: `pub enum LoggingError { DirCreation, SubscriberInit, InvalidRuleName, InvalidEnvPrefix, DuplicateRuleName, FilterParse }`
  Inputs: config validation, env filter parsing, directory creation, and subscriber init failures.
  Output: unified logging setup error.
  Errors: `None`.
  Side Effects: `None`.

- Name: `Rotation`
  Kind: `enum`
  Signature: `pub enum Rotation { Daily, Hourly, Minutely }`
  Inputs: selected in `LogSink::File`.
  Output: file appender rotation policy.
  Errors: `None`.
  Side Effects: `None`.

- Name: `LogLevel`
  Kind: `enum`
  Signature: `pub enum LogLevel { Trace, Debug, Info, Warn, Error }`
  Inputs: string conversion or internal level mapping.
  Output: tracing level value.
  Errors: `None`.
  Side Effects: `None`.

- Name: `LogSink`
  Kind: `enum`
  Signature: `pub enum LogSink { File { dir: String, rotation: Rotation }, Stdout { pretty: bool }, Stderr { pretty: bool } }`
  Inputs: sink destination plus formatting mode.
  Output: one log output target.
  Errors: file sinks can fail at init time.
  Side Effects: may create log directories and write files.

- Name: `LogRule`
  Kind: `struct`
  Signature: `pub struct LogRule { pub name: String, pub sink: LogSink, pub default_filter: String }`
  Inputs: rule name, sink, and fallback filter.
  Output: one independently configurable logging rule.
  Errors: `validate()` returns `LoggingError`.
  Side Effects: environment lookup uses the rule name as suffix.

- Name: `LoggingConf`
  Kind: `struct`
  Signature: `pub struct LoggingConf { pub env_prefix: Option<String>, pub rules: Vec<LogRule> }`
  Inputs: optional env prefix and per-rule config.
  Output: full logging config used by site bootstrap.
  Errors: `validate()` returns `LoggingError`.
  Side Effects: `None`.

- Name: `LoggingGuard`
  Kind: `struct`
  Signature: `pub struct LoggingGuard { _file_guards: Vec<WorkerGuard> }`
  Inputs: returned from tracing initialization.
  Output: keeps non-blocking file writers alive.
  Errors: `None`.
  Side Effects: dropping it ends worker guard lifetime.

- Name: logging methods
  Kind: `fn`
  Signature: `LogRule::validate`, `LoggingConf::resolved_env_prefix`, `LoggingConf::validate`
  Inputs: receiver `&self`.
  Output: validated config or resolved prefix.
  Errors: `LoggingError`.
  Side Effects: `None`.

- Name: `init_tracing`
  Kind: `fn`
  Signature: `pub(crate) fn init_tracing(project_dir: &Path, conf: &LoggingConf) -> Result<LoggingGuard, LoggingError>`
  Inputs: project dir and `LoggingConf`.
  Output: active tracing subscriber and worker guard.
  Errors: `LoggingError`.
  Side Effects: validates config, creates directories, installs the global tracing subscriber.

## Usage Examples

### Example 1

Goal: Send pretty logs to stdout.

```rust
use uxar::logging::{LogRule, LogSink, LoggingConf};

let conf = LoggingConf {
    env_prefix: None,
    rules: vec![LogRule {
        name: "UXAR".into(),
        sink: LogSink::Stdout { pretty: true },
        default_filter: "info".into(),
    }],
};

conf.validate()?;
# Ok::<(), uxar::logging::LoggingError>(())
```

Why valid:

- Rule names and filter strings are validated explicitly.
- `env_prefix: None` resolves to `RUST_LOG`.

### Example 2

Goal: Write JSON logs to rotating files.

```rust
use uxar::logging::{LogRule, LogSink, LoggingConf, Rotation};

let conf = LoggingConf {
    env_prefix: Some("APP_LOG".into()),
    rules: vec![LogRule {
        name: "API".into(),
        sink: LogSink::File {
            dir: "logs".into(),
            rotation: Rotation::Daily,
        },
        default_filter: "warn,my_app=debug".into(),
    }],
};
```

Why valid:

- File sinks require a directory and rotation policy.
- Rule-specific env override will use `APP_LOG_API`.

### Example 3

Goal: Disable a rule through env semantics.

```rust
use uxar::logging::{LogRule, LogSink};

let rule = LogRule {
    name: "UXAR".into(),
    sink: LogSink::Stderr { pretty: false },
    default_filter: "off".into(),
};

rule.validate()?;
# Ok::<(), uxar::logging::LoggingError>(())
```

Why valid:

- `off` is an accepted disabled filter value.
- Disabled rules do not install a tracing layer.

## Behavior Rules

- MUST validate every rule name before initialization.
- MUST reject empty rule names.
- MUST reject rule names longer than 48 characters.
- MUST require rule names to start with an ASCII letter.
- MUST allow only ASCII letters, digits, and `_` after the first character.
- MUST default `env_prefix` to `RUST_LOG` when unset.
- MUST validate `env_prefix` when present.
- MUST require `env_prefix` to start with an uppercase ASCII letter.
- MUST reject duplicate rule names.
- MUST resolve the effective filter in this order: `<PREFIX>_<RULE>`, then `<PREFIX>`, then `default_filter`.
- MUST treat `off`, `0`, `false`, and `no` as disabled values.
- MUST skip layer installation for disabled rules.
- MUST create file sink directories before installing file writers.
- MUST keep `LoggingGuard` alive for non-blocking file sinks.
- MUST install the tracing subscriber only once per process.
- `init_tracing` is site-owned initialization and is not a public standalone bootstrap API.

## Integration Guide

1. Build a `LoggingConf` and attach it to `SiteConf.logging`.
2. Define one or more `LogRule` values with sink and fallback filter.
3. Optionally set `env_prefix` to isolate the app's log env variables.
4. Let site bootstrap call logging initialization during `build_site` or `serve_site`.
5. Set `<PREFIX>` or `<PREFIX>_<RULE>` env vars when deployment needs different filters.
6. Keep logging examples focused on config; do not call `init_tracing` directly from app code unless the crate surface changes.

## Failure Modes

| Condition                                                    | Observed Outcome                  | Fix                                                                        |
| ------------------------------------------------------------ | --------------------------------- | -------------------------------------------------------------------------- |
| Rule name is empty, too long, or contains invalid characters | `LoggingError::InvalidRuleName`   | Use a 1-48 char ASCII identifier starting with a letter.                   |
| Env prefix is invalid                                        | `LoggingError::InvalidEnvPrefix`  | Use uppercase letters, digits, and `_`, starting with an uppercase letter. |
| Two rules use the same name                                  | `LoggingError::DuplicateRuleName` | Rename one rule.                                                           |
| Filter string cannot be parsed by `EnvFilter`                | `LoggingError::FilterParse`       | Use a valid tracing filter directive or a disabled value such as `off`.    |
| File log directory cannot be created                         | `LoggingError::DirCreation`       | Fix the directory path or permissions.                                     |
| Tracing subscriber is already initialized                    | `LoggingError::SubscriberInit`    | Initialize tracing once per process.                                       |

## Non-Goals

- Does not define application business logs or event taxonomy.
- Does not expose `init_tracing` as a public stable bootstrap entrypoint.
- Does not replace observability features such as metrics or tracing export.

## LLM Recipe

1. Build `LoggingConf` through explicit `LogRule` values.
2. Default to `RUST_LOG` unless the app needs a custom env namespace.
3. Use stdout pretty logs for local development examples.
4. Use file sinks only when the example needs persistence or rotation.
5. Generate valid tracing filters, not ad hoc log-level strings.
6. Use disabled values only when the goal is to suppress a rule.
7. Attach logging config to `SiteConf` and let site bootstrap own initialization.
8. Anti-pattern: calling `init_tracing` directly from generated application code while also using `build_site`.
9. Anti-pattern: generating duplicate rule names or lowercase env prefixes.
10. Final check: if a file sink uses a relative path, assume it resolves from `project_dir`.
