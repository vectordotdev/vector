#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};
use semver::Version;

use crate::utils::{deprecation, git, paths};

/// Enact a deprecation: record it as removed and delete the deprecation.d fragment
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
        let dir = repo_root.join(deprecation::DEPRECATION_DIR);

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

        let path = dir.join(&filename);
        if !path.exists() {
            bail!("No deprecation fragment found at {}", path.display());
        }

        // Parse the fragment to get entry data.
        let entries = deprecation::read_deprecation_fragments(&dir)?;
        let entry = entries
            .into_iter()
            .find(|e| e.filename == filename)
            .ok_or_else(|| anyhow::anyhow!("Could not parse {filename}"))?;

        let version = if let Some(v) = self.version {
            // Reject `--version 0.58.0-alpha` and friends: only release-shaped
            // semver is valid here.
            if !v.pre.is_empty() || !v.build.is_empty() {
                bail!(
                    "--version {v} has prerelease or build metadata; only plain X.Y.Z is allowed"
                );
            }
            v
        } else {
            let latest = git::latest_release_version()?;
            Version::new(latest.major, latest.minor + 1, 0)
        };

        if !deprecation::later_minor(&version, &entry.deprecated_since.0) {
            bail!(
                "removed_in ({version}) must be in a later minor release than deprecated_since ({}); \
                 the deprecation policy requires at least one minor release between \
                 the announcement and removal. \
                 Check --version or the fragment's `deprecated_since` field.",
                entry.deprecated_since
            );
        }

        let enacted = deprecation::EnactedEntry {
            what: entry.what.clone(),
            deprecated_since: entry.deprecated_since.to_string(),
            removed_in: version.to_string(),
            description: entry.description.clone(),
        };

        // Append to enacted JSON.
        deprecation::append_enacted(&repo_root, enacted)?;
        println!("Recorded enacted entry for: {}", entry.what);

        // Delete the deprecation.d fragment.
        std::fs::remove_file(&path)?;
        println!("Deleted {}", path.display());

        // Regenerate deprecations.cue.
        deprecation::sync_deprecations_cue(&repo_root)?;
        println!(
            "Updated {}",
            repo_root.join(deprecation::DEPRECATIONS_JSON).display()
        );

        Ok(())
    }
}
