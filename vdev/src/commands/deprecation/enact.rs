#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};
use semver::Version;

use crate::utils::{deprecation, git, paths};

/// Enact a deprecation: move the deprecation.d fragment to the enacted deprecations
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Filename (slug) in deprecation.d/ to enact, e.g. "azure-monitor-logs-sink"
    slug: String,

    /// The Vector version in which this feature was removed, e.g. "0.58.0".
    /// Defaults to the next minor after the latest git tag.
    #[arg(long)]
    version: Option<Version>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;

        // Accept "slug" or "slug.md"; reject path-like inputs to keep the
        // identifier unambiguous (and the file lookup safe).
        if self.slug.contains('/') || self.slug.contains('\\') {
            bail!(
                "expected a deprecation slug (e.g. \"azure-monitor-logs-sink\"), not a path: {}",
                self.slug
            );
        }
        let stem = self.slug.strip_suffix(".md").unwrap_or(&self.slug);
        let filename = format!("{stem}.md");

        let version = if let Some(v) = self.version {
            v
        } else {
            let latest = git::latest_release_version()?;
            Version::new(latest.major, latest.minor + 1, 0)
        };

        let dest = deprecation::enact(&repo_root, &filename, &version)?;
        println!(
            "Moved {}/{filename} to {} (removed in {version})",
            deprecation::DEPRECATION_DIR,
            dest.strip_prefix(&repo_root).unwrap_or(&dest).display()
        );
        Ok(())
    }
}
