# Uxar

Axum-based web framework for Postgres-backed JSON APIs where handler code drives OpenAPI, auth, and query execution.

> ⚠️ **Experimental** — usable, but unstable. APIs may change without notice. Expect breaking changes.

---

## What it is

Uxar explores a handler-first model:

- OpenAPI is derived from handler signatures and doc-comments
- role-based auth is enforced via extractors (`permit!`)
- queries use SQL-shaped builders (`db::select/...`)
- application features are composed as **Bundles**

There is no separate annotation layer for API specs. The code _is_ the spec.

---

## Core ideas

- **Routes + OpenAPI from the same source**  
  Handler signatures, doc-comments, and `permit!` guards generate the spec.

- **`permit!` role guards**  
  Axum extractor enforcing roles at the type level; automatically emits `bearerAuth` in OpenAPI.

- **Bundle composition**  
  A Bundle is Uxar’s unit of composition — a feature can include routes, background jobs, signals, and docs, and be mounted as a single value.

- **Built-in JWT**  
  Access + refresh tokens via HTTP-only cookies or `Authorization` header.

- **SQL-shaped query builders**  
  `db::select/insert/update/delete` return builders directly; bind parameters, filters, and SQL errors are deferred to the terminal async call.

- **Cron + signals**  
  Compile-time validated cron/periodic emitters connected to typed in-process signal handlers.

---

## Example

Full source: [`uxar/examples/notes.rs`](uxar/examples/notes.rs) — login, CRUD routes, cron job, in-process signal.

```
cargo run --example notes
# open http://localhost:8080/v1/api/docs
```

```rust
use uxar::{
    AuthUser, Site, SiteConf, SiteError,
    bundles::{self, Bundle, OpenApiConf},
    db::{Bindable, DBSession, FilteredBuilder, Scannable},
    errors::{Error, ErrorKind},
    permit, roles::BitRole, serve_site,
};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(BitRole)]
pub enum Role { User = 0, Admin = 1 }

/// A note row returned from the database.
#[derive(Debug, Serialize, Deserialize, JsonSchema, Scannable)]
pub struct Note { pub id: i64, pub owner: String, pub title: String, pub body: String }

#[derive(Bindable)]
struct NewNote { owner: String, title: String, body: String }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteInput { pub title: String, pub body: String }

/// List all notes belonging to the authenticated user.
#[bundles::route(path = "/notes")]
async fn list_notes(site: Site, p: permit!(Role, User)) -> Result<Json<Vec<Note>>, Error> {
    let user: AuthUser = p.into_user();
    let mut db = site.db();
    let notes: Vec<Note> = uxar::db::select("notes")
        .filter("owner = :owner")
        .bind_as("owner", user.key.to_string())
        .all(&mut db).await?;;
    Ok(Json(notes))
}

/// Create a note for the authenticated user.
#[bundles::route(path = "/notes", method = "POST")]
async fn create_note(
    site: Site, p: permit!(Role, User), Json(input): Json<NoteInput>,
) -> Result<Json<Note>, Error> {
    let user: AuthUser = p.into_user();
    let mut db = site.db();
    let saved: Note = uxar::db::insert("notes")
        .row(&NewNote { owner: user.key.to_string(), title: input.title, body: input.body })
        .one(&mut db).await?;;
    Ok(Json(saved))
}

#[tokio::main]
async fn main() -> Result<(), SiteError> {
    let bundle = bundles::bundle! { list_notes, create_note }
        .with_openapi(OpenApiConf::default());
    serve_site(SiteConf::from_env_with_files().unwrap(), bundle).await
}
```
