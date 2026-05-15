#![allow(clippy::print_stdout)]

use anyhow::Result;
use semver::Version;

use crate::utils::{
    deprecation::{self, DeprecationEntry},
    paths,
};

/// Show upcoming and in-progress deprecation notices
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Filter to only show entries whose deprecation_version matches this release version.
    #[arg(long)]
    version: Option<Version>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let dir = repo_root.join(deprecation::DEPRECATION_DIR);

        if !dir.is_dir() {
            println!("No {} directory found.", dir.display());
            return Ok(());
        }

        let mut entries = deprecation::read_deprecation_fragments(&dir)?;

        if entries.is_empty() {
            println!("No deprecation notices found.");
            return Ok(());
        }

        if let Some(ref version) = self.version {
            entries.retain(|e| e.deprecation_version.matches_release(version));
            if entries.is_empty() {
                println!("No deprecations enacted in v{version}.");
                return Ok(());
            }
        }

        // Split into enacted (major.minor matches filter) vs planned
        let (enacted, planned): (Vec<_>, Vec<_>) = match &self.version {
            Some(v) => {
                let enacted: Vec<_> = entries
                    .iter()
                    .filter(|e| e.deprecation_version.matches_release(v))
                    .collect();
                let planned: Vec<_> = entries
                    .iter()
                    .filter(|e| !e.deprecation_version.matches_release(v))
                    .collect();
                (enacted, planned)
            }
            None => (vec![], entries.iter().collect()),
        };

        if !enacted.is_empty() {
            println!("Deprecations (enacted in this release):");
            for e in &enacted {
                print_entry(e);
            }
        }

        if !planned.is_empty() {
            if !enacted.is_empty() {
                println!();
            }
            println!("Planned deprecations:");
            for e in &planned {
                print_entry(e);
            }
        }

        Ok(())
    }
}

fn print_entry(e: &DeprecationEntry) {
    println!("  [{}] {}", e.deprecation_version, e.what);
    if let Some(ref ann) = e.announcement_version {
        println!("      announced: {ann}");
    }
    if !e.description.is_empty() {
        // Indent description lines for readability
        for line in e.description.lines() {
            println!("      {line}");
        }
    }
}
