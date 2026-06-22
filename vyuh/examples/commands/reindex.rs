#[path = "../common.rs"] mod example_common;
//! Operational command that rebuilds an in-process search index service.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example commands_reindex search:reindex --full
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use vyuh::{
    Data, Error, Site, SiteConf, bundles,
    commands::CommandConf,
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
async fn reindex(site: Site, Data(args): Data<ReindexArgs>) -> Result<(), Error> {
    let index = site.service::<SearchIndex>().map_err(Error::other)?;
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
    let conf = SiteConf::from_env_with_files()?;
    example_common::run_example_with_conf(conf, bundle).await
}
