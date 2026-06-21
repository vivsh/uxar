//! Operational command that rebuilds an in-process search index service.
//!
//! Run:
//!
//! ```sh
//! cargo run --example commands_reindex search:reindex --full
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use vyuh::{
    Site, SiteConf, bundles,
    commands::{CommandArgs, CommandConf, CommandError},
    services::{Service, ServiceInstance},
};

#[derive(Default)]
struct SearchIndex {
    rebuilds: AtomicUsize,
}

impl SearchIndex {
    fn rebuild(&self, full: bool) -> usize {
        let count = self.rebuilds.fetch_add(1, Ordering::SeqCst) + 1;
        println!(
            "rebuilt {} search index; rebuild_count={count}",
            if full { "full" } else { "incremental" }
        );
        count
    }
}

impl Service for SearchIndex {}

async fn search_index() -> ServiceInstance<SearchIndex> {
    SearchIndex::default().into()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ReindexArgs {
    /// Rebuild every indexed record instead of only changed records.
    #[serde(default)]
    full: bool,
}

/// Rebuild the search index.
async fn reindex(site: Site, args: CommandArgs<ReindexArgs>) -> Result<(), CommandError> {
    let index = site
        .service::<SearchIndex>()
        .map_err(|err| CommandError::Other(Box::new(err)))?;
    index.rebuild(args.full);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle([
        bundles::service(search_index),
        bundles::command(
            reindex,
            CommandConf::new("search:reindex").description("Rebuild the search index."),
        ),
    ]);
    vyuh::run_command(SiteConf::from_env_with_files()?, bundle).await
}
