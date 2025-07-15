use std::path::{Path, PathBuf};
use uxar::embed::{embed, File, Dir, Entry, DirSet};


#[test]
fn test_file_path_and_read() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file_path = root.join("tests/data/embed/foo.txt");
    let file = File::Path { root, path: file_path.clone() };
    assert_eq!(file.base_name(), Some("foo.txt"));
    assert!(!file.is_embedded());
    assert_eq!(file.path(), PathBuf::from("tests/data/embed/foo.txt"));
    let bytes = file.read_bytes_sync().unwrap();
    assert_eq!(bytes, b"hello world\n");
}

#[tokio::test]
async fn test_file_path_async_read() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file_path = root.join("tests/data/embed/foo.txt");
    let file = File::Path { root, path: file_path };
    let bytes = file.read_bytes_async().await.unwrap();
    assert_eq!(bytes, b"hello world\n");
}

#[test]
fn test_file_path_nonexistent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file_path = root.join("uxar/tests/data/embed/does_not_exist.txt");
    let file = File::Path { root, path: file_path };
    assert!(file.read_bytes_sync().is_err());
}

#[test]
fn test_dir_path_entries_and_get_file() {
    let dir = Dir::new("tests/data/embed");
    assert!(!dir.is_embedded());
    let entries = dir.entries();
    assert!(entries.iter().any(|e| e.is_file()));
    let file = dir.get_file("foo.txt");
    assert!(file.is_some());
    let file = file.unwrap();
    assert_eq!(file.base_name(), Some("foo.txt"));
}

#[test]
fn test_entry_and_dirset() {
    let dir = Dir::new("tests/data/embed");
    let entry = Entry::Dir(dir);
    assert!(entry.is_dir());
    let dirset = DirSet::new(vec![Dir::new("tests/data/embed")]);
    let entries = dirset.entries();
    assert!(entries.iter().any(|e| e.is_file()));
    let file = dirset.get_file("foo.txt");
    assert!(file.is_some());
    let files: Vec<_> = dirset.walk().collect();
    assert!(files.iter().any(|f| f.base_name() == Some("foo.txt")));
}

// Macro tests: Embedded and Non-Embedded
#[test]
fn test_macro_non_embedded_dir() {
    let dir = embed!("tests/data/embed", false);
    assert!(!dir.is_embedded());
    let file = dir.get_file("foo.txt");
    assert!(file.is_some());
    let file = file.unwrap();
    let bytes = file.read_bytes_sync().unwrap();
    assert_eq!(bytes, b"hello world\n");
}

#[test]
fn test_macro_non_embedded_file_access() {
    let dir = embed!("tests/data/embed", false);
    let file = dir.get_file("foo.txt").unwrap();
    assert!(!file.is_embedded());
    assert_eq!(file.base_name(), Some("foo.txt"));
}

#[test]
fn test_macro_non_embedded_invalid() {
    let dir = embed!("tests/data/embed", false);
    assert!(dir.get_file("does_not_exist.txt").is_none());
}

#[test]
fn test_macro_embedded_dir() {
    let dir = embed!("tests/data/embed", true);
    assert!(dir.is_embedded());
    let file = dir.get_file("foo.txt");
    assert!(file.is_some());
    let file = file.unwrap();
    let bytes = file.read_bytes_sync().unwrap();
    assert_eq!(bytes, b"hello world\n");
}

#[test]
fn test_macro_embedded_file_access() {
    let dir = embed!("tests/data/embed", true);
    let file = dir.get_file("foo.txt").unwrap();
    assert!(file.is_embedded());
    assert_eq!(file.base_name(), Some("foo.txt"));
}

#[test]
fn test_macro_embedded_invalid() {
    let dir = embed!("tests/data/embed", true);
    assert!(dir.get_file("does_not_exist.txt").is_none());
}

