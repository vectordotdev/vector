use std::{
    env::consts::ARCH,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use directories::ProjectDirs;

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
    } else if cfg!(target_os = "macos") {
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
