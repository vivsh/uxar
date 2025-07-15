use std::collections::VecDeque;
pub use uxar_macros::embed;

pub enum File{
    Embed(include_dir::File<'static>),
    Path(std::path::PathBuf),
}

impl File {

    pub fn base_name(&self) -> Option<&str> {
        self.path().file_name()
            .and_then(|name| name.to_str())
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self, File::Embed(_))
    }

    pub fn path(&self) -> &std::path::Path {
        match self {
            File::Embed(file) => file.path(),
            File::Path(path) => path.as_path(),
        }
    }

    pub fn read_bytes_sync(&self) -> std::io::Result<Vec<u8>> {
        match self {
            File::Embed(file) => Ok(file.contents().to_vec()),
            File::Path(path_buf) => std::fs::read(path_buf),
        }
    }

    pub async fn read_bytes_async(&self) -> std::io::Result<Vec<u8>> {
        match self {
            File::Embed(file) => Ok(file.contents().to_vec()),
            File::Path(path_buf) => tokio::fs::read(path_buf).await,
        }
    }
}


pub enum Dir {
    Embed(include_dir::Dir<'static>),
    Path(std::path::PathBuf),
}

impl Dir {

    pub fn new(path: &'static str) -> Self {
        let base = env!("CARGO_MANIFEST_DIR");
        Dir::Path(std::path::PathBuf::from(base).join(path))
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self, Dir::Embed(_))
    }

    pub fn path(&self) -> &std::path::Path {
        match self {
            Dir::Embed(dir) => dir.path(),
            Dir::Path(path) => path.as_path(),
        }
    }

    pub fn entries(&self) -> Vec<Entry> {
        match self {
            Dir::Embed(dir) => dir.files()
                .map(|file| Entry::File(File::Embed(file.clone())))
                .chain(dir.dirs().map(|subdir| Entry::Dir(Dir::Embed(subdir.clone()))))
                .collect(),
            Dir::Path(path_buf) => {
                let mut entries = Vec::new();
                if let Ok(entries_iter) = std::fs::read_dir(path_buf) {
                    for entry in entries_iter.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            entries.push(Entry::File(File::Path(path)));
                        } else if path.is_dir() {
                            entries.push(Entry::Dir(Dir::Path(path)));
                        }
                    }
                }
                entries
            }
        }
    }

    pub fn get_file(&self, name: &str) -> Option<File> {
        match self {
            Dir::Embed(dir) => dir.get_file(name).map(|file| File::Embed(file.clone())),
            Dir::Path(path_buf) => {
                let path = path_buf.join(name);
                if path.is_file() {
                    Some(File::Path(path))
                } else {
                    None
                }
            }
        }
    }

}

pub enum Entry{
    File(File),
    Dir(Dir),
}

impl Entry {

    pub fn path(&self) -> &std::path::Path {
        match self {
            Entry::File(file) => file.path(),
            Entry::Dir(dir) => dir.path(),
        }
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self, Entry::File(File::Embed(_))) || matches!(self, Entry::Dir(Dir::Embed(_)))
    }

    pub const fn is_file(&self) -> bool {
        matches!(self, Entry::File(_))
    }

    pub const fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir(_))
    }
    
}


pub struct DirSet{
    pub dirs: Vec<Dir>,
}

impl DirSet {

    pub fn new(dirs: Vec<Dir>) -> Self {
        Self { dirs }
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.dirs.iter().flat_map(|dir| dir.entries ()).collect()
    }

    pub fn get_file(&self, name: &str) -> Option<File> {
        for dir in &self.dirs {
            if let Some(file) = dir.get_file(name) {
                return Some(file);
            }
        }
        None
    }

    pub fn walk(&self) -> impl Iterator<Item = File> {
        let mut queue: VecDeque<Entry> = VecDeque::new();
        for dir in &self.dirs {
            for entry in dir.entries() {
                queue.push_back(entry);
            }
        }

        std::iter::from_fn(move || {
            while let Some(entry) = queue.pop_front() {
                match entry {
                    Entry::File(file) => return Some(file),
                    Entry::Dir(dir) => queue.extend(dir.entries().into_iter()),
                }
            }
            None
        })
    }
}


