use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CommandConf, CommandError, CommandRegistry};
use crate::Site;
use crate::callables::specs::{ArgPart, IntoArgPart};
use crate::callables::{self, FromSite, Payload};

// ── SiteRef extractor ─────────────────────────────────────────────────────────

/// Site extractor for use in two-arg command handlers.
pub(crate) struct SiteRef(pub Site);

impl FromSite for SiteRef {
    fn from_site(site: &Site) -> Result<Self, callables::CallError> {
        Ok(SiteRef(site.clone()))
    }
}

impl IntoArgPart for SiteRef {
    fn into_arg_part() -> ArgPart {
        ArgPart::Ignore
    }
}

// ── registry ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ServeArgs {}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct HealthArgs {}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ShowConfigArgs {}

pub fn core_registry() -> Result<CommandRegistry, CommandError> {
    let mut registry = CommandRegistry::new();

    let serve = super::command::<
        ServeArgs,
        _,
        crate::callables::specs::Tuple2<SiteRef, Payload<ServeArgs>>,
    >(
        serve_command,
        CommandConf::new("serve").description("Start the HTTP server."),
    )?;
    registry.register(serve)?;

    let health = super::command::<
        HealthArgs,
        _,
        crate::callables::specs::Tuple2<SiteRef, Payload<HealthArgs>>,
    >(
        health_command,
        CommandConf::new("health").description("Check basic site health."),
    )?;
    registry.register(health)?;

    let show_config = super::command::<
        ShowConfigArgs,
        _,
        crate::callables::specs::Tuple2<SiteRef, Payload<ShowConfigArgs>>,
    >(
        show_config_command,
        CommandConf::new("config").description("Print the effective site configuration."),
    )?;
    registry.register(show_config)?;

    Ok(registry)
}

async fn show_config_command(
    SiteRef(site): SiteRef,
    _args: Payload<ShowConfigArgs>,
) -> Result<(), CommandError> {
    let json =
        serde_json::to_string_pretty(site.conf()).map_err(|e| CommandError::Other(Box::new(e)))?;
    println!("{json}");
    Ok(())
}

// ── handlers ──────────────────────────────────────────────────────────────────

async fn serve_command(
    SiteRef(site): SiteRef,
    _args: Payload<ServeArgs>,
) -> Result<(), CommandError> {
    crate::site::start_site_server(site)
        .await
        .map_err(|e| CommandError::Other(Box::new(e)))
}

async fn health_command(
    SiteRef(site): SiteRef,
    _args: Payload<HealthArgs>,
) -> Result<(), CommandError> {
    let uptime = site.uptime().as_secs();
    let db_ok = sqlx::query("SELECT 1")
        .execute(site.db().as_sqlx())
        .await
        .is_ok();
    println!("health: ok");
    println!("database: {}", if db_ok { "ok" } else { "error" });
    println!("uptime_seconds: {}", uptime);
    Ok(())
}
