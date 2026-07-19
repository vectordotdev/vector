use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::utils::{git, paths};

const CHANGELOG_DIR: &str = "changelog.d";
const DEFAULT_MAX_FRAGMENTS: usize = 1000;

/// Allowed changelog fragment types.
///
/// NOTE: keep this list in sync with `vdev/src/commands/release/generate_cue.rs`
/// and `changelog.d/README.md`.
const FRAGMENT_TYPES: &[&str] = &["breaking", "security", "feature", "enhancement", "fix"];

/// Validate changelog fragments added on this branch/PR.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Merge base to diff against.
    #[arg(long, default_value = "origin/master")]
    merge_base: String,

    /// Maximum number of fragments accepted in a single PR.
    #[arg(long, default_value_t = DEFAULT_MAX_FRAGMENTS)]
    max_fragments: usize,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let changelog_dir = repo_root.join(CHANGELOG_DIR);
        if !changelog_dir.is_dir() {
            bail!(
                "No {CHANGELOG_DIR}/ directory at {}. Run this from the Vector repo root.",
                repo_root.display()
            );
        }

        let fragments = added_fragments(&self.merge_base)?;
        if fragments.is_empty() {
            bail!(
                "No changelog fragments detected. \
                 If no changes necessitate user-facing explanations, add the 'no-changelog' label. \
                 Otherwise, add fragments to {CHANGELOG_DIR}/ (see {CHANGELOG_DIR}/README.md)."
            );
        }
        if fragments.len() > self.max_fragments {
            bail!(
                "Too many changelog fragments ({} > {}).",
                fragments.len(),
                self.max_fragments
            );
        }

        let expected_parent = std::path::Path::new(CHANGELOG_DIR);
        for path in &fragments {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                bail!("Unexpected fragment path: {}", path.display());
            };
            if name == "README.md" {
                continue;
            }
            if path.parent() != Some(expected_parent) {
                bail!(
                    "invalid fragment path '{}': fragments must live directly under {CHANGELOG_DIR}/, not in a subdirectory.",
                    path.display()
                );
            }
            info!("Validating '{name}'");
            validate_filename(name)?;
            validate_contents(&repo_root.join(path), name)?;
        }

        info!("changelog additions are valid.");
        Ok(())
    }
}

/// `git diff --name-only --diff-filter=A --merge-base <merge_base> changelog.d`
fn added_fragments(merge_base: &str) -> Result<Vec<PathBuf>> {
    let out = git::run_and_check_output(&[
        "diff",
        "--name-only",
        "--diff-filter=A",
        "--merge-base",
        merge_base,
        CHANGELOG_DIR,
    ])?;
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(PathBuf::from)
        .collect())
}

fn validate_filename(filename: &str) -> Result<()> {
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() != 3 {
        bail!(
            "invalid fragment filename '{filename}': expected '<unique_name>.<fragment_type>.md'"
        );
    }
    let fragment_type = parts[1];
    if !FRAGMENT_TYPES.contains(&fragment_type) {
        bail!(
            "invalid fragment filename '{filename}': fragment type must be one of ({}).",
            FRAGMENT_TYPES.join("|")
        );
    }
    if parts[2] != "md" {
        bail!("invalid fragment filename '{filename}': extension must be markdown (.md).");
    }
    Ok(())
}

fn validate_contents(path: &std::path::Path, filename: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    // Match generate_cue.rs, which reads `lines().last()` verbatim: the authors
    // line must be the last line, no trailing blank lines allowed.
    let last_line = content.lines().last().unwrap_or("");

    let Some(names) = last_line.strip_prefix("authors: ") else {
        bail!(
            "invalid fragment contents for '{filename}': last line must be 'authors: <name> [<name> ...]' (no trailing blank lines)."
        );
    };
    let names = names.trim();
    if names.is_empty() {
        bail!("invalid fragment contents for '{filename}': authors line has no names.");
    }
    if names.contains('@') {
        bail!(
            "invalid fragment contents for '{filename}': author names should not be prefixed with '@'."
        );
    }
    if names.contains(',') {
        bail!(
            "invalid fragment contents for '{filename}': authors should be space delimited, not comma delimited."
        );
    }
    Ok(())
}
