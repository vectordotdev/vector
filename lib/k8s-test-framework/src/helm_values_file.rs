use std::path::Path;

use crate::temp_file::TempFile;

#[derive(Debug)]
pub struct HelmValuesFile(TempFile);

impl HelmValuesFile {
    pub fn new(data: &str) -> std::io::Result<Self> {
        Ok(Self(TempFile::new("values.yml", data)?))
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}
