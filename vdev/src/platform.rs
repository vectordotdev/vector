use cached::proc_macro::once;
use directories::ProjectDirs;
use std::env::consts::ARCH;
use std::path::PathBuf;

pub fn canonicalize_path(path: &String) -> String {
    match dunce::canonicalize(path) {
        Ok(p) => p.display().to_string(),
        Err(_) => path.to_string(),
    }
}

pub fn home() -> PathBuf {
    match home::home_dir() {
        Some(path) => path,
        None => ["~"].iter().collect(),
    }
}

pub fn data_dir() -> PathBuf {
    match _project_dirs() {
        Some(path) => path.data_local_dir().to_path_buf(),
        None => [home().to_str().unwrap(), ".local", "vector", "vdev"]
            .iter()
            .collect(),
    }
}

pub fn default_target() -> String {
    if windows() {
        format!("{}-pc-windows-msvc", ARCH)
    } else if macos() {
        format!("{}-apple-darwin", ARCH)
    } else {
        format!("{}-unknown-linux-gnu", ARCH)
    }
}

pub const fn windows() -> bool {
    cfg!(windows)
}

#[allow(dead_code)]
pub const fn macos() -> bool {
    cfg!(target_os = "macos")
}

#[allow(dead_code)]
pub const fn unix() -> bool {
    cfg!(not(any(windows, target_os = "macos")))
}

#[once]
fn _project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "vector", "vdev")
}
