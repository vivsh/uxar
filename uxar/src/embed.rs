
use rust_silos::{Silo, SiloSet, File as SiloFile};
use std::io::Read;
use std::path::Path;

/// Wrapper around rust-silos File with sync/async read methods
pub struct File {
    inner: SiloFile,
}

impl File {
    fn new(inner: SiloFile) -> Self {
        Self { inner }
    }

    pub fn base_name(&self) -> Option<&str> {
        self.inner.path().file_name()?.to_str()
    }

    pub fn is_embedded(&self) -> bool {
        self.inner.is_embedded()
    }

    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    pub fn read_bytes_sync(&self) -> std::io::Result<Vec<u8>> {
        let mut reader = self.inner.reader().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub async fn read_bytes_async(&self) -> std::io::Result<Vec<u8>> {
        self.read_bytes_sync()
    }
}

/// Wrapper around rust-silos Silo
pub struct Dir {
    inner: Silo,
}

impl Dir {
    pub fn empty() -> Self {
        Self {
            inner: Silo::new(""),
        }
    }

    pub fn new(path: &str) -> Self {
        let base = env!("CARGO_MANIFEST_DIR");
        let full = std::path::PathBuf::from(base).join(path);
        Self {
            inner: Silo::new(full.to_str().unwrap_or(path)),
        }
    }

    pub fn is_embedded(&self) -> bool {
        self.inner.is_embedded()
    }

    pub fn path(&self) -> &Path {
        Path::new("")
    }

    pub fn get_file(&self, name: &str) -> Option<File> {
        self.inner.get_file(name).map(File::new)
    }
}

impl From<Silo> for Dir {
    fn from(silo: Silo) -> Self {
        Self { inner: silo }
    }
}

/// Collection of directories with overlay support
pub struct DirSet {
    inner: SiloSet,
}

impl DirSet {
    pub fn new(dirs: Vec<Dir>) -> Self {
        let silos: Vec<Silo> = dirs.into_iter().map(|d| d.inner).collect();
        Self {
            inner: SiloSet::new(silos),
        }
    }

    pub fn get_file(&self, name: &str) -> Option<File> {
        self.inner.get_file(name).map(File::new)
    }

    pub fn walk(&self) -> impl Iterator<Item = File> {
        let files: Vec<File> = self.inner.iter().map(File::new).collect();
        files.into_iter()
    }
}


