use sqlx::Row as _;
use vyuh::db::{DbConf, DbPool};

#[tokio::main]
async fn main() -> Result<(), vyuh::db::DbError> {
    let conf = DbConf::from_env()?;
    let pool = DbPool::from_conf(&conf).await?;

    let row = sqlx::query("SELECT COUNT(*) AS total FROM notes")
        .fetch_one(pool.as_sqlx())
        .await?;
    let total: i64 = row.try_get("total")?;

    println!("notes: {total}");
    Ok(())
}
