use std::{fs::OpenOptions, io::Write, path::PathBuf};

use tempfile::TempDir;
use vector::test_util::temp_file;

#[cfg(feature = "cli-tests")]
mod cli;

#[cfg(feature = "shutdown-tests")]
mod shutdown;

/// Creates a file with given content
#[must_use]
pub fn create_file(config: &str) -> PathBuf {
    let path = temp_file();
    overwrite_file(path.clone(), config);
    path
}

/// Creates a YAML config file (`.yaml` extension so Vector parses it as YAML)
#[must_use]
pub fn create_yaml_file(config: &str) -> PathBuf {
    let mut path = temp_file();
    path.set_extension("yaml");
    overwrite_file(path.clone(), config);
    path
}

/// Overwrites file with given content.
///
/// # Panics
///
/// Panics if the file cannot be opened, written, or synchronized.
pub fn overwrite_file(path: PathBuf, config: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.sync_all().unwrap();
}

/// Creates a temporary directory.
///
/// # Panics
///
/// Panics if the directory cannot be created.
#[must_use]
pub fn create_directory() -> TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}
