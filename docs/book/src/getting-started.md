# Getting Started

This chapter walks through a small Vyuh application that touches the framework
surfaces most projects care about first: routes, JWT auth, a task, a command,
a cron emitter, a signal handler, and OpenAPI.

It is not a production-ready application, and it is not trying to hide that.
The goal is simpler: show how Vyuh keeps HTTP, background work, scheduled work,
and operations in one model without making them feel like separate systems.

## Data Types

Start with ordinary Rust types.

`Data<T>` is the main wrapper Vyuh moves through handlers. Add `Validate` when
input should be checked at the boundary. Add `JsonSchema` when a type should
appear in generated OpenAPI.

```rust
use vyuh::prelude::*;

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
struct LoginOut {
    access: String,
    refresh: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct BuildReportJob {
    account_id: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct ReportBuilt {
    location: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct Tick {
    source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct ReindexArgs {
    scope: String,
}
```

These types are intentionally plain. Vyuh does not ask you to declare a second
schema layer just to move data through the framework.

## Routes

Routes are ordinary async functions with typed inputs and typed outputs. The
important thing to notice is not the macro. It is the function signature.

```rust
use vyuh::prelude::*;
use vyuh::auth::{AuthUser, TokenPair};

#[bundles::route(path = "/users", method = "POST")]
async fn signup(Valid(Data(input)): Valid<Data<Signup>>) -> Result<Data<UserCreated>, Error> {
    Ok(Data::new(UserCreated {
        id: 1,
        email: input.email.clone(),
        name: input.name.clone(),
    }))
}

#[bundles::route(path = "/login", method = "POST")]
async fn login(site: Site) -> Result<Data<LoginOut>, Error> {
    let tokens: TokenPair = site
        .auth()
        .create_token_pair(AuthUser::new("user-123", 0), &["web"])?;

    Ok(Data::new(LoginOut {
        access: tokens.access,
        refresh: tokens.refresh,
    }))
}

#[bundles::route(path = "/me", method = "GET")]
async fn me(user: AuthUser) -> Result<Data<String>, Error> {
    Ok(Data::new(user.key.to_string()))
}
```

`signup` is a validated JSON route. `login` and `me` show the normal JWT path
with very little ceremony.

That combination is already most of what a real API does: parse input, validate
it, authenticate some endpoints, and issue tokens without dropping into
untyped request plumbing.

## Task, Cron, And Command

Tasks, cron emitters, signals, and commands are different runtime paths, but
Vyuh does not make them feel alien. They use the same handler style and the
same bundle composition model.

```rust
use vyuh::prelude::*;

#[bundles::task]
async fn build_report(Data(job): Data<BuildReportJob>) -> Data<ReportBuilt> {
    Data::new(ReportBuilt {
        location: format!("reports/{}.json", job.account_id),
    })
}

#[bundles::cron(expr = "0 */5 * * * *")]
async fn heartbeat() -> Data<Tick> {
    Data::new(Tick {
        source: "docs-example".into(),
    })
}

#[bundles::signal]
async fn record_tick(Data(tick): Data<Tick>) -> Result<(), Error> {
    println!("tick from {}", tick.source);
    Ok(())
}

async fn rebuild_index(Data(args): Data<ReindexArgs>) -> Result<(), Error> {
    println!("reindex {}", args.scope);
    Ok(())
}
```

This is where Vyuh starts to feel different. The route, task, cron emitter,
signal handler, and command are not the same feature, but they are close enough
in shape that you can move between them without switching mental models.

- The route handles HTTP input.
- The task handles durable background input.
- The cron handler runs on a schedule and emits typed data.
- The signal handler receives that emitted data in-process.
- The command handles CLI input.

All of them are ordinary async functions over typed data. That is the
uniformity argument in practice, not as a slogan.

## Auth And OpenAPI

Auth stays explicit. If a handler does not extract auth, Vyuh does no auth work
for it. For a first application, JWT is the path with the least ceremony:
configure auth once, issue a token pair, and extract `AuthUser` where a route
should require an access token.

OpenAPI works the same way Vyuh usually works: attach it once at the bundle,
and let it follow the routes that bundle already owns. That means prefixes,
nesting, and route metadata stay aligned without a parallel documentation tree.

```rust
use vyuh::auth::AuthConf;

let auth = AuthConf::default();

fn api_bundle() -> bundles::Bundle {
    bundles::bundle! {
        signup,
        login,
        me,
        build_report,
        heartbeat,
        record_tick,
    }
    .merge(command_bundle())
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Vyuh Getting Started")
            .version("0.1.0")
            .description("Routes, auth, tasks, commands, and cron.")
            .spec("/openapi.json")
            .viewer("/docs"),
    )
    .with_prefix("/api")
}
```

OpenAPI and the docs viewer need no per-route schema file, no separate route
table, and no duplicate metadata layer. They come from the handlers and bundle
declaration you already had to write anyway.

## Command Bundle

Commands are registered directly and then merged like any other bundle part.
That matters because commands are operational code, but they still belong to
the same application.

```rust
use vyuh::bundles;
use vyuh::commands::CommandConf;

fn command_bundle() -> bundles::Bundle {
    bundles::bundle([bundles::command(
        rebuild_index,
        CommandConf::new("search:rebuild").description("Rebuild the search index."),
    )])
}
```

The result is modest but useful: CLI work stays close to the feature that owns
it instead of drifting into a separate operational codebase.

## Main Function

Put the bundle and site configuration together in `main`. This is where the
different surfaces stop being examples and become one application.

```rust
use vyuh::prelude::*;
use vyuh::auth::AuthConf;

#[tokio::main]
async fn main() -> Result<(), SiteError> {
    let auth = AuthConf::default();

    Site::run(
        SiteConf::from_env_with_files()?
            .auth(auth),
        api_bundle(),
    )
    .await
}
```

The bundle is where the feature surface comes together: routes, a task, a cron
emitter, a signal handler, command registration, JWT-protected handlers, and
OpenAPI. `main` stays small because the feature wiring already lives with the
feature.

## What To Notice

- One bundle holds the feature surface instead of scattering setup across
  unrelated registries.
- `Data<T>` keeps the route, task, signal, command, and cron shapes
  recognizably close to each other.
- Validation is explicit through `Valid<Data<T>>`, not inferred from derives
  alone.
- JWT auth is explicit through `AuthUser`, while setup stays small with
  `AuthConf::default()`.
- OpenAPI is attached once and follows the bundle tree automatically.

From here, the next useful pages are [Bundles](bundles.md), [Routes](routes.md),
[OpenAPI](openapi.md), [Tasks](tasks.md), and [Auth](auth.md).
