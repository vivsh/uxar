#[path = "../common.rs"] mod example_common;

use vyuh::db::mock::MockDBSession;
use vyuh::db::{self, FilteredBuilder};

#[derive(Debug, vyuh::db::Bindable)]
struct NewNote {
    title: String,
    done: bool,
}

#[derive(Debug, vyuh::db::Bindable)]
struct NotePatch {
    done: bool,
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let mut db = MockDBSession::new();
    db.plan_execute_ok("INSERT INTO notes", 1);
    db.plan_execute_ok("UPDATE notes", 1);

    let note = NewNote {
        title: "write database docs".to_string(),
        done: false,
    };
    db::insert("notes").row(&note).execute(&mut db).await?;

    let patch = NotePatch { done: true };
    db::update("notes")
        .set(&patch)
        .filter("id = :id")
        .bind_as("id", 1_i64)
        .execute(&mut db)
        .await?;

    println!("inserted and updated a note");
    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example(bundle).await
}

