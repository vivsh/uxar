use vyuh::db::mock::{DbCallKind, MockDBSession, PlannedCall, PlannedResponse};
use vyuh::db::{self, FilteredBuilder};

#[derive(Debug, Clone, vyuh::db::Scannable)]
struct Note {
    id: i64,
    title: String,
    done: bool,
}

#[tokio::main]
async fn main() -> Result<(), vyuh::db::DbError> {
    let mut db = MockDBSession::new();
    db.plan(PlannedCall {
        kind: DbCallKind::FetchAll,
        sql_contains: Some("SELECT id, title, done FROM notes"),
        response: PlannedResponse::OkAnyVec(Box::new(vec![Note {
            id: 1,
            title: "ship vyuh".to_string(),
            done: false,
        }])),
    });

    let notes: Vec<Note> = db::select("notes")
        .filter("done = :done")
        .bind_as("done", false)
        .order_by("id", true)
        .all(&mut db)
        .await?;

    for note in notes {
        println!("#{} {} done={}", note.id, note.title, note.done);
    }
    Ok(())
}
