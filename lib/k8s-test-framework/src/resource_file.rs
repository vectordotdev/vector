use std::path::Path;

use crate::temp_file::TempFile;

#[derive(Debug)]
pub struct ResourceFile(TempFile);

impl ResourceFile {
    pub fn new(data: &str) -> std::io::Result<Self> {
        Ok(Self(TempFile::new("custom.yaml", data)?))
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}
