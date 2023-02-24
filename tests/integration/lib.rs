use std::{
    fs::{create_dir, OpenOptions},
    io::Write,
    path::PathBuf,
};

use vector::test_util::{temp_dir, temp_file};

#[cfg(feature = "cli-tests")]
mod cli;

#[cfg(feature = "shutdown-tests")]
mod shutdown;

/// Creates a file with given content
pub fn create_file(config: &str) -> PathBuf {
    let path = temp_file();
    overwrite_file(path.clone(), config);
    path
}

/// Overwrites file with given content
pub fn overwrite_file(path: PathBuf, config: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
}

pub fn create_directory() -> PathBuf {
    let path = temp_dir();
    create_dir(path.clone()).unwrap();
    path
}
