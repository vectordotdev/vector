use crate::git::added_files_against_merge_base;
use crate::path_utils::get_changelog_dir;
use anyhow::{anyhow, Context, Result};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const FRAGMENT_TYPES: &[&str] = &[
    "breaking",
    "security",
    "deprecation",
    "feature",
    "enhancement",
    "fix",
];

const DEFAULT_MAX_FRAGMENTS: usize = 1000;

/// Validate changelog fragments added in this branch/PR.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Changelog directory (defaults to $CHANGELOG_DIR or "changelog.d")
    #[arg(long)]
    changelog_dir: Option<PathBuf>,

    /// Merge base to diff against (defaults to $MERGE_BASE or "origin/master")
    #[arg(long)]
    merge_base: Option<String>,

    /// Max fragments threshold (defaults to $MAX_FRAGMENTS or 1000)
    #[arg(long)]
    max_fragments: Option<usize>,
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        let changelog_dir = self
            .changelog_dir.clone().unwrap_or_else(|| get_changelog_dir());

        if !changelog_dir.is_dir() {
            error!(
                "No ./{} found. This tool must be invoked from the root of the repo.",
                changelog_dir.display()
            );
            std::process::exit(1);
        }

        let merge_base = self
            .merge_base.clone()
            .or_else(|| env::var("MERGE_BASE").ok())
            .unwrap_or_else(|| "origin/master".to_string());

        let max_fragments: usize = self
            .max_fragments
            .or_else(|| env::var("MAX_FRAGMENTS").ok().and_then(|s| s.parse().ok()))
            .unwrap_or(DEFAULT_MAX_FRAGMENTS);

        let fragments = added_files_against_merge_base(&merge_base, &changelog_dir)
            .context("failed to collect added changelog fragments")?;

        if fragments.is_empty() {
            info!("No changelog fragments detected");
            info!("If no changes necessitate user-facing explanations, add the GH label 'no-changelog'");
            info!("Otherwise, add changelog fragments to {}/", changelog_dir.display());
            info!("For details, see '{}/README.md'", changelog_dir.display());
            std::process::exit(1);
        }

        if fragments.len() > max_fragments {
            error!("Too many changelog fragments ({} > {}).", fragments.len(), max_fragments);
            std::process::exit(1);
        }

        for path in fragments {
            if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                if name == "README.md" {
                    continue;
                }
                info!("validating '{}'", name);
                self.validate_fragment_filename(name)?;
                self.validate_fragment_contents(&changelog_dir.join(name), name)?;
            } else {
                return Err(anyhow!("unexpected path (no filename): {}", path.display()));
            }
        }

        info!("changelog additions are valid.");
        Ok(())
    }

    fn validate_fragment_filename(&self, fname: &str) -> Result<()> {
        // Expected: <unique_name>.<fragment_type>.md
        let parts: Vec<&str> = fname.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow!(
            "invalid fragment filename: wrong number of period delimiters. \
             expected '<unique_name>.<fragment_type>.md'. ({})",
            fname
        ));
        }

        let fragment_type = parts[1];
        if !FRAGMENT_TYPES.contains(&fragment_type) {
            return Err(anyhow!(
            "invalid fragment filename: fragment type must be one of: ({}). ({})",
            FRAGMENT_TYPES.join("|"),
            fname
        ));
        }

        if parts[2] != "md" {
            return Err(anyhow!(
            "invalid fragment filename: extension must be markdown (.md): ({})",
            fname
        ));
        }

        Ok(())
    }

    fn validate_fragment_contents(&self, path: &Path, fname: &str) -> Result<()> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read fragment file: {}",
                path.to_string_lossy()
            )
        })?;

        // Emulate `tail -n 1` (last line; may be empty if file ends with newline)
        let last_line = content
            .rsplit_once('\n')
            .map(|(_, tail)| tail)
            .or_else(|| content.lines().last())
            .unwrap_or(&content); // single-line file without newline

        // Accept either "author:" or "authors:" (case-sensitive), followed by one-or-more non-space
        // Example valid lines:
        //   authors: Alice Bob
        //   author: Alice
        let trimmed = last_line.trim_end_matches(['\r', '\n']);
        if !(trimmed.starts_with("authors:") || trimmed.starts_with("author:")) {
            return Err(anyhow!(
            "invalid fragment contents: last line must start with 'author: ' or 'authors: ' and include at least one name. ({})",
            fname
        ));
        }

        // Split on the first colon, take the remainder as names
        let names = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();

        if names.is_empty() {
            return Err(anyhow!(
            "invalid fragment contents: author line is empty. ({})",
            fname
        ));
        }

        if names.contains('@') {
            return Err(anyhow!(
            "invalid fragment contents: author should not be prefixed with @"
        ));
        }

        if names.contains(',') {
            return Err(anyhow!(
            "invalid fragment contents: authors should be space delimited, not comma delimited."
        ));
        }

        Ok(())
    }
}
