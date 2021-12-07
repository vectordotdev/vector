use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

#[derive(Debug)]
pub struct TempFile {
    dir: TempDir,
    path: PathBuf,
}

impl TempFile {
    pub fn new(file_name: &str, data: &str) -> std::io::Result<Self> {
        let dir = tempdir()?;
        let path = dir.path().join(file_name);
        std::fs::write(&path, data)?;
        Ok(Self { dir, path })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}
