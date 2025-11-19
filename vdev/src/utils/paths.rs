//! Path-related utilities

use std::{
    env,
    fmt::Debug,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

/// Find the Vector repository root by searching upward for markers like .git or Cargo.toml
/// with a `[workspace]` section.
pub fn find_repo_root() -> Result<PathBuf> {
    let mut current = env::current_dir().context("Could not determine current directory")?;

    loop {
        // Check for .git directory (most reliable marker)
        if current.join(".git").is_dir() {
            return Ok(current);
        }

        // Check for Cargo.toml with workspace (Vector's root Cargo.toml has [workspace])
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.is_file()
            && let Ok(contents) = fs::read_to_string(&cargo_toml)
            && contents.contains("[workspace]")
        {
            return Ok(current);
        }

        // Move up one directory
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            anyhow::bail!(
                "Could not find Vector repository root. Please run vdev from within the Vector repository."
            );
        }
    }
}

/// Check if a path exists
pub fn exists(path: impl AsRef<Path> + Debug) -> Result<bool> {
    match fs::metadata(path.as_ref()) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context(format!("Could not stat {path:?}")),
    }
}
