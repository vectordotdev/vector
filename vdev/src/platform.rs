use directories::ProjectDirs;

use std::env::consts::ARCH;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn canonicalize_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    dunce::canonicalize(path)
        .unwrap_or_else(|err| panic!("Could not canonicalize path {path:?}: {err}"))
        .display()
        .to_string()
}

pub fn data_dir() -> &'static Path {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
    DATA_DIR.get_or_init(|| {
        ProjectDirs::from("", "vector", "vdev")
            .expect("Could not determine the project directory")
            .data_local_dir()
            .to_path_buf()
    })
}

pub fn default_target() -> String {
    if cfg!(windows) {
        format!("{ARCH}-pc-windows-msvc")
    } else if cfg!(macos) {
        format!("{ARCH}-apple-darwin")
    } else {
        format!("{ARCH}-unknown-linux-gnu")
    }
}

pub fn default_features() -> &'static str {
    if cfg!(windows) {
        "default-msvc"
    } else {
        "default"
    }
}
