# Commands

Vyuh commands are site-aware CLI entrypoints. Use them for administration,
diagnostics, maintenance, one-off data repair, and local operational tools that
should run against the same configured site as the web server.

Commands are not durable background work. Use [Tasks](tasks.md) for retryable
work that must survive restarts, and [Services](services.md) for site-lifetime
background loops.

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
- `CommandArgs<T>` for typed command arguments.
- `run_command(conf, bundle)` for CLI entrypoints.
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
    bundles,
    commands::{CommandArgs, CommandConf, CommandError},
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct GreetArgs {
    name: String,
    #[serde(default)]
    loud: bool,
}

async fn greet(args: CommandArgs<GreetArgs>) -> Result<(), CommandError> {
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

Use `run_command` as the normal CLI entrypoint:

```rust
#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    vyuh::run_command(vyuh::SiteConf::from_env_with_files()?, app_bundle()).await
}
```

Then run commands by name:

```sh
cargo run -- greet --name Vyuh --loud
cargo run -- help
cargo run -- greet --help
```

Built-in commands include `help`, `serve`, `health`, and `config`.

`run_command` returns an error when site build or command execution fails. With
a normal `#[tokio::main] async fn main() -> Result<_, _>`, success exits with
code `0`, while site build failures and command failures exit non-zero.

Each command invocation runs one command in that process. Vyuh does not take a
global command lock, so separate processes may run commands concurrently. Use
database locks, transactions, advisory locks, or application-level coordination
when an operation must be exclusive.

## Arguments

Command arguments come from the payload type's `JsonSchema`. Keep command
argument structs simple and object-shaped:

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

`CommandArgs<T>` supports `Deref`, `AsRef`, and `into_inner()`:

```rust
async fn greet(args: CommandArgs<GreetArgs>) -> Result<(), CommandError> {
    let args = args.into_inner();
    println!("hello {}", args.name);
    Ok(())
}
```

## Site-Aware Commands

Extract `Site` when a command needs subsystem access:

```rust
use vyuh::{Site, commands::{CommandArgs, CommandError}};

async fn reindex(site: Site, args: CommandArgs<ReindexArgs>) -> Result<(), CommandError> {
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
async fn rebuild(site: Site, args: CommandArgs<RebuildArgs>) -> Result<(), CommandError> {
    site.tasks()
        .submit_with(RebuildIndex { full: args.full }, Default::default())
        .await
        .map_err(|err| CommandError::Other(Box::new(err)))?;
    Ok(())
}
```

Use this when the command should trigger durable work and return quickly. Do the
work directly in the command only when it is naturally short-lived and
operationally interactive.

Commands do not automatically run inside a database transaction. Use the normal
database/session/transaction APIs explicitly when an operation needs atomicity.

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
- duplicate command names and reserved names fail site build.

## Router Boundary

Commands do not need raw router access. Use `serve_site` or the built-in
`serve` command for serving, and `vyuh::testing::router(&site)` only for tests
or interop that truly needs an Axum `Router`.

## Future Extensions

The command schema gives Vyuh room to add richer CLI features without changing
handler signatures:

- structured `CommandOutput` renderers such as JSON, YAML, and tables;
- shell completion generation;
- richer command grouping for namespaced command sets.

## Examples

- [`commands_basic.rs`](../vyuh/examples/commands_basic.rs): typed command args
  and direct registration.
- [`commands_site.rs`](../vyuh/examples/commands_site.rs): a command that
  extracts `Site` and reads site configuration.
- [`commands_reindex.rs`](../vyuh/examples/commands_reindex.rs): an
  operational command that rebuilds an in-process search index service.

## Current Limitations

- Commands are in-process and scoped to one built site.
- Commands are not durable, retried, scheduled, or supervised.
- Argument parsing intentionally supports a small predictable flag syntax.
- Commands should stay short-lived; long-running background behavior belongs in
  services or tasks.
- Macro sugar for commands is deferred; direct registration is the supported
  API in this pass.
