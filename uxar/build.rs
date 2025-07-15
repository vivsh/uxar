// build.rs: Ensure testdata/foo.txt exists at compile time for embedding
use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let dir = Path::new("testdata");
    if !dir.exists() {
        fs::create_dir_all(&dir).unwrap();
    }
    let file = dir.join("foo.txt");
    if !file.exists() {
        let mut f = fs::File::create(&file).unwrap();
        f.write_all(b"hello world").unwrap();
    }
}
