#[path = "../common.rs"] mod example_common;

use vyuh::db::mock::MockDBSession;
use vyuh::db::{DBSession, Statement};

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let mut db = MockDBSession::new();
    db.plan_fetch_scalar_ok("COUNT(*)", 3_i64);

    let total: i64 = db
        .fetch_scalar(Statement::from_str("SELECT COUNT(*) FROM notes WHERE done = $1").bind(false))
        .await?;

    println!("open notes: {total}");
    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example(bundle).await
}

