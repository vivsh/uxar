//! Getting Started — Notes API (uxar / Rust)
//!
//! Equivalent to the FastAPI and DRF examples under `examples/cmp`:
//! same endpoints, same JWT cookie auth, same role model, same cron flow,
//! same OpenAPI docs.
//!
//! Run:
//!   DATABASE_URL=postgres://user:pass@localhost/notes_db \
//!   SECRET_KEY=change-me-in-production \
//!   cargo run --example notes
//!
//! OpenAPI docs: http://localhost:8080/v1/api/docs

use axum::{Json, response::Response};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uxar::auth::BitRole as _;
use uxar::{
    apidocs::DocViewer,
    auth,
    bundles,
    callables::Payload,
    db::{Bindable, FilteredBuilder, Scannable},
    errors::{Error, ErrorKind},
    Site, SiteConf, SiteError,
    serve_site,
};

#[derive(auth::BitRole)]
pub enum Role {
    User = 0,
    Admin = 1,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Scannable)]
pub struct Note {
    pub id: i64,
    pub owner: String,
    pub title: String,
    pub body: String,
}

#[derive(Bindable)]
struct NewNote {
    owner: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteInput {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PruneEvent {
    pub triggered_by: String,
}

/// Authenticate; sets JWT access + refresh cookies on success.
#[bundles::route(path = "/login", method = "POST")]
async fn login(site: Site, Json(req): Json<LoginReq>) -> Result<Response, Error> {
    // TODO: verify against your users table with a hashed password check.
    if req.username != "alice" {
        return Err(Error::new(ErrorKind::Unauthorized));
    }

    let stored_hash = auth::make_password("secret", Some("notes-demo-salt"), Some("pbkdf2_sha256"))?;
    let valid = auth::check_password(&req.password, &stored_hash)?;
    if !valid {
        return Err(Error::new(ErrorKind::Unauthorized));
    }

    let user = auth::AuthUser::new(&req.username, <Role as auth::BitRole>::to_role_type(Role::User));

    let mut resp = Response::default();
    site.authenticator().login_user(user, &[], &mut resp)?;
    
    Ok(resp)
}

/// List all notes belonging to the authenticated user.
#[bundles::route(path = "/notes")]
async fn list_notes(site: Site, p: auth::permit!(Role, User)) -> Result<Json<Vec<Note>>, Error> {
    let user: auth::AuthUser = p.into_user();
    let mut db = site.db();
    let notes: Vec<Note> = uxar::db::select("notes")
        .filter("owner = :owner")
        .bind_as("owner", user.key.to_string())
        .all(&mut db)
        .await?;
    Ok(Json(notes))
}

/// Create a note; returns the saved note with its id.
#[bundles::route(path = "/notes", method = "POST")]
async fn create_note(
    site: Site,
    p: auth::permit!(Role, User),
    Json(input): Json<NoteInput>,
) -> Result<Json<Note>, Error> {
    let user: auth::AuthUser = p.into_user();
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

/// Delete all notes. Requires Admin role.
#[bundles::route(path = "/notes/all", method = "DELETE")]
async fn purge_notes(site: Site, _p: auth::permit!(Role, Admin)) -> Result<Json<u64>, Error> {
    let mut db = site.db();
    let deleted = uxar::db::delete("notes")
        .execute(&mut db)
        .await?;
    Ok(Json(deleted))
}

/// Fire every night at midnight (extend this to run cleanup queries).
#[bundles::cron(expr = "0 0 0 * * *")]
async fn nightly_prune() -> Payload<PruneEvent> {
    PruneEvent { triggered_by: "cron".into() }.into()
}

/// Handle the nightly prune signal.
#[bundles::signal]
async fn on_prune(event: Payload<PruneEvent>) {
    tracing::info!(triggered_by = %event.triggered_by, "nightly prune fired");
}

#[tokio::main]
async fn main() -> Result<(), SiteError> {
    let bundle = bundles::bundle! {
        login,
        list_notes,
        create_note,
        purge_notes,
        nightly_prune,
        on_prune
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Notes API")
            .description("Getting-started example for uxar")
            .version("0.1.0")
            .spec("/api/openapi.json")
            .doc("/api/docs")
            .viewer(DocViewer::Rapidoc),
    );

    let conf = SiteConf::from_env_with_files()
        .expect("set DATABASE_URL and SECRET_KEY in .env");

    serve_site(conf, bundle.with_prefix("/v1")).await
}
