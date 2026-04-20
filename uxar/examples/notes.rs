//! Getting Started — Notes API
//!
//! A minimal JSON API that demonstrates the core uxar features:
//!   • GET and POST routes with OpenAPI docs auto-generated from handler signatures
//!   • Role-based access control via `permit!` (enforced at the type level)
//!   • Login endpoint that issues JWT tokens and sets HTTP-only cookies
//!   • AuthUser extraction from a role-gated handler
//!   • A real Postgres query with the new typed builder API
//!   • A nightly cron job wired to an in-process signal handler
//!
//! # Running
//! ```
//! cargo run --example notes
//! ```
//!
//! # Required env vars (put in a `.env` file)
//! ```
//! DATABASE_URL=postgres://user:pass@localhost/notes_db
//! SECRET_KEY=change-me-in-production
//! ```
//!
//! Browse the live API docs at http://localhost:8080/v1/api/docs once running.

use axum::{Json, response::Response};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uxar::{
    AuthUser, Site, SiteConf, SiteError,
    apidocs::{ApiMeta, DocViewer},
    bundles::{self, Bundle, OpenApiConf},
    callables::Payload,
    db::{Bindable, DBSession, FilteredBuilder, Scannable},
    errors::{Error, ErrorKind},
    permit,
    roles::BitRole,
    serve_site,
};
// ── Roles ─────────────────────────────────────────────────────────────────────

#[derive(BitRole)]
pub enum Role {
    /// Regular authenticated user.
    User = 0,
    /// Administrator — can perform destructive operations.
    Admin = 1,
}

// ── Models ────────────────────────────────────────────────────────────────────

/// A note as stored in and returned from the database.
#[derive(Debug, Serialize, Deserialize, JsonSchema, Scannable)]
pub struct Note {
    pub id: i64,
    pub owner: String,
    pub title: String,
    pub body: String,
}

/// Fields supplied when creating a new note; `id` and `owner` are set by the server.
#[derive(Bindable)]
struct NewNote {
    owner: String,
    title: String,
    body: String,
}

/// Request body for `POST /notes`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteInput {
    pub title: String,
    pub body: String,
}

/// Request body for `POST /login`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

/// Payload carried by the nightly-prune cron signal.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PruneEvent {
    pub triggered_by: String,
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/// Authenticate with username + password; returns JWT cookies on success.
///
/// On success the response sets `access_token` and `refresh_token` HTTP-only
/// cookies.  All subsequent requests authenticate via those cookies or the
/// `Authorization: JWT <token>` header.
///
/// > **Note:** replace the credential check with a real DB lookup in production.
#[bundles::route(path = "/login", method = "POST")]
async fn login(site: Site, Json(req): Json<LoginReq>) -> Result<Response, Error> {
    // TODO: verify `req.username` + hashed password against your `users` table.
    if req.username != "alice" || req.password != "secret" {
        return Err(Error::new(ErrorKind::Unauthorized));
    }

    let user = AuthUser::new(&req.username, Role::User.to_role_type());
    let pair = site.authenticator().create_token_pair(user, &[])?;

    let mut resp = Response::default();
    site.authenticator().login(&pair.access_token, false, &mut resp);
    site.authenticator().login(&pair.refresh_token, true, &mut resp);
    Ok(resp)
}

// ── Routes ────────────────────────────────────────────────────────────────────

/// List all notes belonging to the authenticated user.
#[bundles::route(path = "/notes")]
async fn list_notes(site: Site, p: permit!(Role, User)) -> Result<Json<Vec<Note>>, Error> {
    let user: AuthUser = p.into_user();
    let mut db = site.db();
    let notes: Vec<Note> = uxar::db::select("notes")
        .filter("owner = :owner")
        .bind_as("owner", user.key.to_string())
        .all(&mut db)
        .await?;
    Ok(Json(notes))
}

/// Create a new note for the authenticated user; returns the saved note with its id.
#[bundles::route(path = "/notes", method = "POST")]
async fn create_note(
    site: Site,
    p: permit!(Role, User),
    Json(input): Json<NoteInput>,
) -> Result<Json<Note>, Error> {
    let user: AuthUser = p.into_user();
    let new_note = NewNote {
        owner: user.key.to_string(),
        title: input.title,
        body: input.body,
    };
    let mut db = site.db();
    let saved: Note = uxar::db::insert("notes")
        .row(&new_note)
        .one(&mut db)
        .await?;
    Ok(Json(saved))
}

/// Delete every note in the system.  Requires the `Admin` role.
#[bundles::route(path = "/notes/all", method = "DELETE")]
async fn purge_notes(site: Site, _p: permit!(Role, Admin)) -> Result<Json<u64>, Error> {
    let mut db = site.db();
    let deleted = uxar::db::delete("notes")
        .execute(&mut db)
        .await?;
    Ok(Json(deleted))
}

// ── Cron + Signal ─────────────────────────────────────────────────────────────

/// Emit a prune event every night at midnight (server timezone).
#[bundles::cron(expr = "0 0 0 * * *")]
async fn nightly_prune() -> Payload<PruneEvent> {
    PruneEvent { triggered_by: "cron".into() }.into()
}

/// Handle the nightly prune signal (extend this to run cleanup queries).
#[bundles::signal]
async fn on_prune(event: Payload<PruneEvent>) {
    tracing::info!(triggered_by = %event.triggered_by, "nightly prune fired");
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), SiteError> {
    let bundle: Bundle = bundles::bundle! {
        login,
        list_notes,
        create_note,
        purge_notes,
        nightly_prune,
        on_prune
    }
    .with_openapi(OpenApiConf {
        doc_path: "/api/docs".into(),
        spec_path: "/api/openapi.json".into(),
        meta: ApiMeta {
            title: "Notes API".into(),
            description: Some("Getting-started example for uxar".into()),
            version: "0.1.0".into(),
            ..Default::default()
        },
        viewer: DocViewer::Rapidoc,
    });

    let conf = SiteConf::from_env_with_files()
        .expect("set DATABASE_URL and SECRET_KEY in .env");

    serve_site(conf, bundle.with_prefix("/v1")).await
}
