#[path = "../common.rs"] mod example_common;

use vyuh::db::{self, DbConf, DbPool, FilteredBuilder};

#[derive(Debug, vyuh::db::Bindable)]
struct NewTodo {
    title: String,
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = DbConf::from_env()?;
    let pool = DbPool::from_conf(&conf).await?;
    let mut tx = pool.begin().await?;

    let todo = NewTodo {
        title: "commit through a transaction".to_string(),
    };

    db::insert("todos").row(&todo).execute(&mut tx).await?;
    db::delete("todos")
        .filter("title = :title")
        .bind_as("title", todo.title)
        .execute(&mut tx)
        .await?;

    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example_with_conf(conf, bundle).await
}

