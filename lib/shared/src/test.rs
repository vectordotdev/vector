use std::{fs::File, path::Path};

pub fn open_fixture(path: impl AsRef<Path>) -> crate::Result<serde_json::Value> {
    let test_file = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(e.into()),
    };
    let value: serde_json::Value = serde_json::from_reader(test_file)?;
    Ok(value)
}
