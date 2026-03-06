use crate::testing::mock_db;

#[tokio::test]
#[cfg(feature = "postgres")]
#[ignore] // Run with: cargo test -- --ignored --nocapture
async fn test_mock_db_creates_and_drops_database() {
    let db = mock_db().await;
    
    // Verify we can create a table
    sqlx::query("CREATE TABLE test_users (id SERIAL PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&*db)
        .await
        .expect("Failed to create table");
    
    // Insert test data
    sqlx::query("INSERT INTO test_users (name) VALUES ($1)")
        .bind("Alice")
        .execute(&*db)
        .await
        .expect("Failed to insert data");
    
    // Query the data back
    let row: (i32, String) = sqlx::query_as("SELECT id, name FROM test_users WHERE name = $1")
        .bind("Alice")
        .fetch_one(&*db)
        .await
        .expect("Failed to fetch data");
    
    assert_eq!(row.1, "Alice");
    
    // Store db_name for verification after drop
    let db_name = db.db_name.clone();
    let base_url = db.base_url.clone();
    
    // Drop the database by dropping the guard
    drop(db);
    
    // Give it a moment for async cleanup
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // Verify the database was dropped by trying to connect to it
    let test_url = format!("{}/{}", base_url, db_name);
    let result = sqlx::PgPool::connect(&test_url).await;
    
    // Should fail because database no longer exists
    assert!(result.is_err(), "Database should have been dropped");
}
