use crate::git::added_files_against_merge_base;
use crate::path_utils::get_changelog_dir;
use anyhow::{anyhow, Context, Result};
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
    /// Changelog directory (defaults to `changelog.d`)
    #[arg(long)]
    changelog_dir: Option<PathBuf>,

    /// Merge base to diff against (defaults to `origin/master`)
    #[arg(long)]
    merge_base: Option<String>,

    /// Max fragments threshold (defaults to `1000`)
    #[arg(long)]
    max_fragments: Option<usize>,
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        let changelog_dir = self
            .changelog_dir.clone().unwrap_or_else(get_changelog_dir);

        if !changelog_dir.is_dir() {
            error!(
                "No ./{} found. This tool must be invoked from the root of the repo.",
                changelog_dir.display()
            );
            std::process::exit(1);
        }

        let merge_base = self
            .merge_base.clone()
            .unwrap_or_else(|| "origin/master".to_string());

        let max_fragments: usize = self
            .max_fragments
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
            error!("Too many changelog fragments ({} > {max_fragments}).", fragments.len());
            std::process::exit(1);
        }

        for path in fragments {
            if let Some(name) = path.file_name().and_then(OsStr::to_str) {
                if name == "README.md" {
                    continue;
                }
                info!("Validating `{name}`");
                validate_fragment_filename(name)?;
                validate_fragment_contents(&changelog_dir.join(name), name)?;
            } else {
                return Err(anyhow!("unexpected path (no filename): {}", path.display()));
            }
        }

        info!("changelog additions are valid.");
        Ok(())
    }
}

fn validate_fragment_filename(filename: &str) -> Result<()> {
    // Expected: <unique_name>.<fragment_type>.md
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!(
            "invalid fragment filename: {filename} - wrong number of period delimiters. \
             expected '<unique_name>.<fragment_type>.md'",
        ));
    }

    let fragment_type = parts[1];
    if !FRAGMENT_TYPES.contains(&fragment_type) {
        return Err(anyhow!(
            "invalid fragment filename: {filename} - fragment type must be one of: {})",
            FRAGMENT_TYPES.join("|"),
        ));
    }

    if parts[2] != "md" {
        return Err(anyhow!(
            "invalid fragment filename: {filename} - extension must be markdown (.md)",
        ));
    }

    Ok(())
}

fn validate_fragment_contents(path: &Path, filename: &str) -> Result<()> {
    let content = std::fs::read_to_string(path).with_context(|| {
        format!("failed to read fragment file: {}", path.to_string_lossy())
    })?;

    // Use last non-empty line to avoid false negatives due to trailing newline(s).
    let last_line = content.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");

    if !last_line.starts_with("authors:") {
        return Err(anyhow!("last line must start with 'authors: ' and include at least one name. ({filename})"));
    }

    // Split on the first colon, take the remainder as names
    let names = last_line.split_once(':').map(|(_, rest)| rest.trim()).unwrap_or("");


    if names.is_empty() {
        return Err(anyhow!(
            "author line is empty. ({})",
            filename
        ));
    }

    if names.contains('@') {
        return Err(anyhow!("author should not be prefixed with @"));
    }

    if names.contains(',') {
        return Err(anyhow!("authors should be space delimited, not comma delimited."));
    }

    Ok(())
}
