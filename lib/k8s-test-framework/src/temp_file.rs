use std::path::{Path, PathBuf};

use tempfile::tempdir;

#[derive(Debug)]
pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    pub fn new(file_name: &str, data: &str) -> std::io::Result<Self> {
        let dir = tempdir()?.into_path();
        let path = dir.join(file_name);
        std::fs::write(&path, data)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(dir) = self.path.parent() {
            _ = std::fs::remove_dir_all(dir);
        }
    }
}
