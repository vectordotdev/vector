#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};
use semver::Version;

use crate::utils::{deprecation, paths};

/// Enact a deprecation: record it as removed and delete the deprecation.d fragment
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Filename (slug) in deprecation.d/ to enact, e.g. "azure-monitor-logs-sink"
    slug: String,

    /// The Vector version in which this feature was removed, e.g. "0.58.0"
    #[arg(long)]
    version: Version,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let dir = repo_root.join(deprecation::DEPRECATION_DIR);

        // Accept "slug", "slug.md", or a path like "deprecation.d/slug.md"
        let slug_path = std::path::Path::new(&self.slug);
        let stem = slug_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.slug);
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

        let enacted = deprecation::EnactedEntry {
            what: entry.what.clone(),
            deprecated_since: entry.deprecated_since.to_string(),
            removed_in: format!("{}.{}.0", self.version.major, self.version.minor),
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
