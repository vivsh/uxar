use std::sync::Arc;

use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uxar::{
    Site, SiteConf, SiteError, admin, bundles::{self, Bundle, CronConf, PeriodicConf, SignalConf}, callables::{self, PatchOp, Payload}, emitters, routes::{Methods, RouteConf}, apidocs::{ApiMeta, DocViewer}, serve_site, services::ServiceRunner  
};

#[bundles::route(
    path = "/bleat",    
)]
/// A simple route that returns a bleat.
/// 
/// Returns "Blee!" as JSON.
/// Random comment for description
async fn bleat(site: Site) -> Json<&'static str> {
    Json("Blee!")
}

async fn greet(path: axum::extract::Path<String>) -> Json<String> {
    Json(format!("Hello, {}!", path.0))
}

#[derive(JsonSchema, Debug, Deserialize, Serialize)]
struct Reminder;

#[derive(Debug, JsonSchema, Deserialize, Serialize)]
struct Chime;

async fn daily_chime() -> Payload<Chime> {
    println!("Chime at {}", chrono::Utc::now());
    Chime.into()
}

async fn on_chime(c: Payload<Chime>) {
    println!("Received chime at {}", chrono::Utc::now());
}

async fn remind_me() -> Payload<Reminder> {
    println!("Reminder emitted at {}", chrono::Utc::now());
    Reminder.into()
}

async fn on_remind(r: Payload<Reminder>) {
    println!("Received reminder at {}", chrono::Utc::now());
}

async fn test_auth() -> Json<&'static str> {
    Json("Authenticated!")
}

use uxar::services::{Agent, Service, ServiceError};

pub trait EmailService: Send + Sync + 'static {
    fn send_email(&self, to: &str, subject: &str, _body: &str) -> Result<(), String>;
}

pub struct EmailServiceImpl;

impl Service for EmailServiceImpl {
    fn run(&mut self, r: &mut ServiceRunner) -> Result<(), ServiceError>
    {
        // expose as DI interface once From<Arc<EmailServiceImpl>> for Arc<dyn EmailService> exists
        // r.expose::<dyn EmailService>()?;
        r.run("email-loop", || async move {
            // background work here
            Ok(())
        });
        Ok(())
    }
}

impl EmailService for EmailServiceImpl {
    fn send_email(&self, to: &str, subject: &str, _body: &str) -> Result<(), String> {
        Ok(())
    }
}

#[bundles::service]
async fn email_service() -> Agent<EmailServiceImpl> {
    EmailServiceImpl.into()
}


#[tokio::main]
async fn main() -> Result<(), SiteError> {
    let b2 = bundles::bundle! {
        email_service,
        bleat
    };    
    let bundle: Bundle = uxar::bundles::bundle([
        uxar::bundles::route(
            greet,
            RouteConf {
                name: "greet".into(),
                path: "/greet/{path}".into(),
                methods: Methods::GET.or(Methods::POST),
            },
        )
        .patch(
            PatchOp::new()
                .description("Hello to the description")
                .arg(0)
                .name("tail")
                .done(),
        ),
        uxar::bundles::cron(
            daily_chime,
            CronConf {
                expr: "0 0 * * * *".into(),
                target: emitters::EmitTarget::Signal,
            },
        ),
        uxar::bundles::periodic(
            remind_me,
            PeriodicConf {
                interval: tokio::time::Duration::from_secs(3),
                target: emitters::EmitTarget::Signal,
            },
        )
        .patch(
            callables::PatchOp::new()
                .description("Hello World")
                .name("some name"),
        ),
        uxar::bundles::signal(on_chime, SignalConf::default()),
        uxar::bundles::signal(on_remind, SignalConf::default()),
        uxar::bundles::route(
            test_auth,
            RouteConf {
                name: "test_auth".into(),
                path: "/test_auth".into(),
                ..Default::default()
            },
        ),
    ])
    .merge(b2)
    .merge(admin::admin_bundle().with_prefix("/admin"))
    .with_openapi(uxar::bundles::OpenApiConf {
        doc_path: "/api/docs".into(),
        spec_path: "/api/openapi.json".into(),
        meta: ApiMeta {
            title: "UXAR Example API".into(),
            description: Some("An example API using UXAR".into()),
            version: "0.1.0".into(),
            ..Default::default()
        },
        viewer: DocViewer::Rapidoc,
    });

    

    let conf = SiteConf::from_env_with_files().expect("Failed to load site configuration from environment");
    println!("Conf: {conf:#?}");

    serve_site(conf, bundle.with_prefix("/v1")).await
}
