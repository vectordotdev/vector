use cached::proc_macro::once;
use dunce;
use home;
use os_info;
use std::env::consts::ARCH;
use std::path::{Path, PathBuf};

#[once]
fn _os_info() -> os_info::Info {
    os_info::get()
}

pub struct Platform {}

impl Platform {
    pub fn new() -> Platform {
        Platform {}
    }

    pub fn canonicalize_path(&self, path: &String) -> String {
        match dunce::canonicalize(Path::new(&path)) {
            Ok(p) => p.display().to_string(),
            Err(_) => path.to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn home(&self) -> PathBuf {
        match home::home_dir() {
            Some(path) => path,
            None => ["~"].iter().collect(),
        }
    }

    pub fn default_target(&self) -> String {
        if self.windows() {
            format!("{}-pc-windows-msvc", ARCH)
        } else if self.macos() {
            format!("{}-apple-darwin", ARCH)
        } else {
            format!("{}-unknown-linux-gnu", ARCH)
        }
    }

    pub const fn windows(&self) -> bool {
        cfg!(target_os = "windows")
    }

    #[allow(dead_code)]
    pub const fn macos(&self) -> bool {
        cfg!(target_os = "macos")
    }

    #[allow(dead_code)]
    pub const fn unix(&self) -> bool {
        cfg!(not(any(target_os = "windows", target_os = "macos")))
    }

    #[allow(dead_code)]
    pub fn os_type(&self) -> os_info::Type {
        _os_info().os_type()
    }
}
