use std::path::{Path, PathBuf};

pub fn get_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn get_changelog_dir() -> PathBuf {
    let root = get_repo_root();
    root.join("changelog.d")
}
