use uxar::db::{extract_tags, SqlTag};
use std::path::Path;

#[tokio::test]
async fn test_extract_tags_users() {
    let tags = extract_tags("tests/data/sql/users.sql").unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].tag, "user_list");
    assert_eq!(tags[0].sql, "SELECT * FROM users;");
    assert_eq!(tags[1].tag, "user_by_id");
    assert!(tags[1].meta.is_some());
    assert!(tags[1].sql.contains("WHERE id = $1"));
}

#[tokio::test]
async fn test_extract_tags_products() {
    let tags = extract_tags("tests/data/sql/products.sql").unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].tag, "all_products");
    assert_eq!(tags[1].tag, "product_by_sku");
    assert!(tags[1].meta.is_some());
}

#[tokio::test]
async fn test_extract_tags_orders() {
    let tags = extract_tags("tests/data/sql/orders.sql").unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].tag, "recent_orders");
    assert!(tags[0].sql.contains("ORDER BY created_at DESC"));
    assert_eq!(tags[1].tag, "order_by_id");
}

#[tokio::test]
async fn test_extract_tags_misc() {
    let tags = extract_tags("tests/data/sql/misc.sql").unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].tag, "ping");
    assert_eq!(tags[1].tag, "complex_query");
    assert!(tags[1].meta.is_some());
}

#[tokio::test]
async fn test_extract_tags_empty() {
    let tags = extract_tags("tests/data/sql/empty.sql").unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag, "empty_tag");
    assert_eq!(tags[0].sql, "-- just a tag, no SQL");
}
