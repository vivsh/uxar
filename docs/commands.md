# Commands

Vyuh commands are site-aware CLI entrypoints. Use them for administration,
diagnostics, maintenance, one-off data repair, and local operational tools that
should run against the same configured site as the web server.

Commands are not durable background work. Use [Tasks](tasks.md) for retryable
work that must survive restarts, and [Services](services.md) for site-lifetime
background loops.

Use commands for one-shot operational actions that need the configured `Site`.
Do not use commands for user-facing HTTP endpoints, retryable background jobs,
or long-running supervised workers.

## Mental Model

- Routes handle HTTP requests.
- Commands handle administration, diagnostics, and maintenance.
- Tasks handle durable background work with retry, sleep, and resume.
- Services handle site-lifetime components and background loops.

| Subsystem | Trigger | Lifetime | Use For | Not For |
| --- | --- | --- | --- | --- |
| Routes | HTTP request | one request | APIs, pages, webhooks | maintenance scripts |
| Commands | CLI invocation | one process command | admin, repair, reindex, diagnostics | durable background jobs |
| Tasks | task submission | persisted work unit | retryable async work, sleeps, external resume | interactive CLI tools |
| Services | site startup | site lifetime | shared clients, caches, in-process loops | one-off operations |

## Overview

The main public pieces are:

- `bundles::command(handler, CommandConf)` for registration.
- `CommandConf::new(name)` for naming the command.
- `Data<T>` for typed command arguments.
- `Site::run(conf, bundle)` for command-aware application entrypoints.
- `Site::execute_command(name, args)` for tests and internal execution.

Commands are registered through bundles and execute against a fully built
`Site`. That means command handlers can extract `Site` and use the same
database, templates, services, tasks, logging, and configuration as routes.
Service constructors have completed and service workers have been spawned before
the command handler runs.

## Registration

Define a typed argument struct and register the command as a bundle part:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, Error, bundles,
    commands::CommandConf,
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct GreetArgs {
    name: String,
    #[serde(default)]
    loud: bool,
}

async fn greet(Data(args): Data<GreetArgs>) -> Result<(), Error> {
    let message = format!("hello {}", args.name);
    println!("{}", if args.loud { message.to_uppercase() } else { message });
    Ok(())
}

let bundle = bundles::bundle([bundles::command(
    greet,
    CommandConf::new("greet").description("Print a greeting."),
)]);
```

Command names must be unique. The reserved `help` command is provided by Vyuh.
Flat names are the primary API today, but namespaced names such as
`user:create`, `search:reindex`, and `db:repair` are a good convention for
larger applications.

## Running

Use `Site::run` as the normal command-aware application entrypoint. With no
command arguments it runs the built-in `serve` command:

```rust
#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    vyuh::Site::run(vyuh::SiteConf::from_env_with_files()?, app_bundle()).await
}
```

Then run commands by name:

```sh
cargo run -p vyuh --no-default-features --features sqlite --example commands_macro -- greet --name Vyuh --loud
cargo run -p vyuh --no-default-features --features sqlite --example commands_site -- help
cargo run -p vyuh --no-default-features --features sqlite --example commands_macro -- greet --help
```

Built-in commands include `help`, `serve`, `health`, and `config`.

`Site::run` returns an error when site build or command execution fails. With
a normal `#[tokio::main] async fn main() -> Result<_, _>`, success exits with
code `0`, while site build failures and command failures exit non-zero.

Each command invocation runs one command in that process. Vyuh does not take a
global command lock, so separate processes may run commands concurrently. Use
database locks, transactions, advisory locks, or application-level coordination
when an operation must be exclusive.

## Arguments

Command arguments come from the data type's `JsonSchema`. Keep command argument
structs simple and object-shaped:

- strings: `--name Vyuh`
- integers and numbers: `--limit 10`
- booleans: `--verbose`, `--verbose true`, or `--no-verbose`
- arrays: repeated values after one flag, such as `--tag api web admin`
- optional fields: omitted when absent
- required fields: reported as missing when not supplied

Unsupported schema shapes fail during site build instead of silently producing a
command with no arguments.

Array values may also be split across repeated flags:

```sh
search:reindex --tag api web --tag admin
```

Empty arrays are not represented with a flag; omit an optional array field when
there are no values. Empty strings are accepted when the shell passes an empty
argument, for example `--name ""`.

`Data<T>` stores an `Arc<T>` so the same wrapper can be shared across
subsystems. It supports pattern matching, `Deref`, `AsRef`, and `into_inner()`:

```rust
async fn greet(Data(args): Data<GreetArgs>) -> Result<(), vyuh::Error> {
    println!("hello {}", args.name);
    Ok(())
}
```

## Site-Aware Commands

Extract `Site` when a command needs subsystem access:

```rust
use vyuh::{Data, Site};

async fn reindex(site: Site, Data(args): Data<ReindexArgs>) -> Result<(), vyuh::Error> {
    let db = site.db();
    let templates = site.templates();
    let tasks = site.tasks();
    Ok(())
}
```

The full site is available because commands run after site build. Service
constructors are different: they run while the site is still being assembled.

Commands may enqueue tasks and this is often a good pattern:

```rust
async fn rebuild(site: Site, Data(args): Data<RebuildArgs>) -> Result<(), vyuh::Error> {
    site.tasks()
        .submit_with(RebuildIndex { full: args.full }, Default::default())
        .await
        .map_err(vyuh::Error::other)?;
    Ok(())
}
```

Use this when the command should trigger durable work and return quickly. Do the
work directly in the command only when it is naturally short-lived and
operationally interactive.

Commands do not automatically run inside a database transaction. Use the normal
database/session/transaction APIs explicitly when an operation needs atomicity.

## Validation

Wrap command data in `Valid<Data<T>>` when CLI arguments should be validated
with the same rules used by routes:

```rust
use vyuh::{Data, Error, Valid, Validate};

#[derive(serde::Deserialize, serde::Serialize, schemars::JsonSchema, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
    #[validate(min_length = 3)]
    name: String,
}

async fn create_user(Valid(Data(args)): Valid<Data<CreateUser>>) -> Result<(), Error> {
    println!("creating {}", args.email);
    Ok(())
}
```

Argument parsing errors are command errors. Validation failures keep their
field-oriented structure and are rendered as CLI output:

```text
Validation failed for command 'create-user':

  --email
    Enter a valid email address.

Use 'create-user --help' for usage.
```

See [Validation](validation.md) for validation rules and [Errors](errors.md)
for the application/subsystem error boundary.

## Help And Errors

`help` lists registered commands. `<command> --help` shows the flags derived
from the command argument schema and field descriptions when available.
`CommandConf::description(...)` overrides the handler doc-comment summary in
help output:

```rust
bundles::command(
    reindex,
    CommandConf::new("search:reindex").description("Rebuild the search index."),
)
```

Commands and flags are shown in deterministic alphabetical order.

Command errors are explicit:

- unknown commands mention `help`;
- unknown flags include the command and flag name;
- missing required arguments name the flag;
- parse errors include the flag, supplied value, and expected type;
- validation failures render field-oriented CLI output;
- handler `vyuh::Error` values render compact application messages;
- duplicate command names and reserved names fail site build.

`CommandError` is for command machinery. Application command handlers should
return `vyuh::Error`.

## Router Boundary

Commands do not need raw router access. Use `Site::serve` for server-only
binaries or the built-in `serve` command through `Site::run`, and use
`vyuh::testing::router(&site)` only for tests or interop that truly needs an
Axum `Router`.

## Future Extensions

The command schema gives Vyuh room to add richer CLI features without changing
handler signatures:

- structured `CommandOutput` renderers such as JSON, YAML, and tables;
- shell completion generation;
- richer command grouping for namespaced command sets.

## Examples

- [`commands_macro.rs`](../vyuh/examples/commands/macro.rs): typed command args
  and direct registration.
- [`commands_site.rs`](../vyuh/examples/commands/site.rs): a command that
  extracts `Site` and reads site configuration.
- [`commands_reindex.rs`](../vyuh/examples/commands/reindex.rs): an
  operational command that rebuilds an in-process search index service.

## Current Limitations

- Commands are in-process and scoped to one built site.
- Commands are not durable, retried, scheduled, or supervised.
- Argument parsing intentionally supports a small predictable flag syntax.
- Commands should stay short-lived; long-running background behavior belongs in
  services or tasks.
- Macro sugar for commands is deferred; direct registration is the supported
  API in this pass.
