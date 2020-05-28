use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

#[derive(Debug)]
pub struct ResourceFile {
    dir: TempDir,
    path: PathBuf,
}

impl ResourceFile {
    pub fn new(data: &str) -> std::io::Result<Self> {
        let dir = tempdir()?;
        let path = dir.path().join("custom.yaml");
        std::fs::write(&path, data)?;
        Ok(Self { dir, path })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for ResourceFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).expect("unable to clean up custom resource file");
    }
}
