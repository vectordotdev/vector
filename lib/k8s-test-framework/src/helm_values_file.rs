use std::path::Path;

use log::info;

use crate::temp_file::TempFile;

#[derive(Debug)]
pub struct HelmValuesFile(TempFile);

impl HelmValuesFile {
    pub fn new(data: &str) -> std::io::Result<Self> {
        info!("Using values \n {}", data);
        Ok(Self(TempFile::new("values.yml", data)?))
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}
