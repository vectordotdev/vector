use cached::proc_macro::once;
use directories::ProjectDirs;

use std::env::consts::ARCH;
use std::path::{Path, PathBuf};

pub fn canonicalize_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    dunce::canonicalize(path)
        .unwrap_or_else(|err| panic!("Could not canonicalize path {path:?}: {err}"))
        .display()
        .to_string()
}

#[once]
pub fn data_dir() -> PathBuf {
    _project_dirs().data_local_dir().to_path_buf()
}

#[once]
pub fn default_target() -> String {
    if cfg!(windows) {
        format!("{}-pc-windows-msvc", ARCH)
    } else if cfg!(macos) {
        format!("{}-apple-darwin", ARCH)
    } else {
        format!("{}-unknown-linux-gnu", ARCH)
    }
}

#[once]
fn _project_dirs() -> ProjectDirs {
    ProjectDirs::from("", "vector", "vdev").expect("Could not determine the project directory")
}
