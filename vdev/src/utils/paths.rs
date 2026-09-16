//! Path-related utilities

use std::{
    env,
    fmt::Debug,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

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

/// Resolve a user-provided path only when it stays below the repository root.
pub fn resolve_repo_relative_path(
    repo_root: &Path,
    path: &Path,
    description: &str,
) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(component) => relative.push(component),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "{description} must be a relative descendant of the repository: {}",
                    path.display()
                );
            }
        }
    }

    if relative.as_os_str().is_empty() {
        bail!(
            "{description} must not be the repository root: {}",
            path.display()
        );
    }

    let output = repo_root.join(relative);
    let canonical_root = repo_root.canonicalize().with_context(|| {
        format!(
            "Could not canonicalize repository root {}",
            repo_root.display()
        )
    })?;
    let existing_ancestor = output
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .expect("repository root must be an existing output ancestor");
    let canonical_ancestor = existing_ancestor.canonicalize().with_context(|| {
        format!(
            "Could not canonicalize output ancestor {}",
            existing_ancestor.display()
        )
    })?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        bail!(
            "{description} must resolve below the repository root: {}",
            path.display()
        );
    }

    Ok(output)
}

/// Find an npm tool installed by `scripts/environment/prepare.sh`.
pub fn npm_tool_path(repo_root: &Path, tool: &str) -> Result<PathBuf> {
    let path = repo_root
        .join("scripts/environment/npm-tools/node_modules/.bin")
        .join(tool);
    if path.is_file() {
        return Ok(path);
    }

    bail!(
        "Could not find {tool} at {}. Run `scripts/environment/prepare.sh --modules={tool}`.",
        path.display()
    )
}

/// Check if a path exists
pub fn exists(path: impl AsRef<Path> + Debug) -> Result<bool> {
    match fs::metadata(path.as_ref()) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context(format!("Could not stat {path:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_relative_path_must_be_a_descendant() {
        let temporary = tempfile::tempdir().unwrap();
        let repo_root = temporary.path().join("repo");
        fs::create_dir(&repo_root).unwrap();

        assert_eq!(
            resolve_repo_relative_path(
                &repo_root,
                Path::new("website/generated/example-configs"),
                "Example output directory",
            )
            .unwrap(),
            repo_root.join("website/generated/example-configs")
        );
        for path in [
            Path::new("."),
            Path::new(".."),
            Path::new("../examples"),
            Path::new("/tmp/examples"),
        ] {
            assert!(
                resolve_repo_relative_path(&repo_root, path, "Example output directory").is_err()
            );
        }
    }

    #[test]
    fn npm_tool_path_requires_the_prepared_tool() {
        let temporary = tempfile::tempdir().unwrap();
        let repo_root = temporary.path().join("repo");
        let tool = repo_root.join("scripts/environment/npm-tools/node_modules/.bin/prettier");
        fs::create_dir_all(tool.parent().unwrap()).unwrap();
        fs::write(&tool, "").unwrap();

        assert_eq!(npm_tool_path(&repo_root, "prettier").unwrap(), tool);
        assert!(npm_tool_path(&repo_root, "markdownlint-cli2").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn repo_relative_path_rejects_symlink_escapes() {
        let temporary = tempfile::tempdir().unwrap();
        let repo_root = temporary.path().join("repo");
        let external = temporary.path().join("external");
        fs::create_dir(&repo_root).unwrap();
        fs::create_dir(&external).unwrap();
        std::os::unix::fs::symlink(&external, repo_root.join("linked")).unwrap();

        assert!(
            resolve_repo_relative_path(
                &repo_root,
                Path::new("linked/output"),
                "Example output directory",
            )
            .is_err()
        );
    }
}
