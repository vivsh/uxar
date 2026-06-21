use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CommandConf, CommandError, CommandRegistry};
use crate::callables::specs::{ArgPart, IntoArgPart};
use crate::callables::{self, Data, FromSite};
use crate::{Error, Site};

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

    let serve =
        super::command::<ServeArgs, _, crate::callables::specs::Tuple2<SiteRef, Data<ServeArgs>>>(
            serve_command,
            CommandConf::new("serve").description("Start the HTTP server."),
        )?;
    registry.register(serve)?;

    let health = super::command::<
        HealthArgs,
        _,
        crate::callables::specs::Tuple2<SiteRef, Data<HealthArgs>>,
    >(
        health_command,
        CommandConf::new("health").description("Check basic site health."),
    )?;
    registry.register(health)?;

    let show_config = super::command::<
        ShowConfigArgs,
        _,
        crate::callables::specs::Tuple2<SiteRef, Data<ShowConfigArgs>>,
    >(
        show_config_command,
        CommandConf::new("config").description("Print the effective site configuration."),
    )?;
    registry.register(show_config)?;

    Ok(registry)
}

async fn show_config_command(
    SiteRef(site): SiteRef,
    _args: Data<ShowConfigArgs>,
) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(site.conf())?;
    println!("{json}");
    Ok(())
}

// ── handlers ──────────────────────────────────────────────────────────────────

async fn serve_command(SiteRef(site): SiteRef, _args: Data<ServeArgs>) -> Result<(), Error> {
    site.start().await.map_err(Error::other)
}

async fn health_command(SiteRef(site): SiteRef, _args: Data<HealthArgs>) -> Result<(), Error> {
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
