use std::fs::File;
use std::path::Path;

pub fn open_fixture(path: impl AsRef<Path>) -> crate::Result<serde_json::Value> {
    serde_json::from_reader(File::open(path)?).map_err(Into::into)
}
