#![allow(clippy::print_stdout)]

use anyhow::Result;

use crate::utils::{deprecation, paths};

/// Check deprecation.d fragments are valid and regenerate generated/deprecations.json
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let dir = repo_root.join(deprecation::DEPRECATION_DIR);

        if !dir.is_dir() {
            println!(
                "No {dir} directory found; nothing to validate.",
                dir = dir.display()
            );
            return Ok(());
        }

        let entries = deprecation::read_deprecation_fragments(&dir)?;

        if entries.is_empty() {
            println!("No deprecation fragments found in {}.", dir.display());
        } else {
            for entry in &entries {
                println!("  ok  {}", entry.filename);
            }
            println!("{} deprecation fragment(s) are valid.", entries.len());
        }

        deprecation::sync_deprecations_cue(&repo_root)?;
        println!(
            "Wrote {}",
            repo_root.join(deprecation::DEPRECATIONS_JSON).display()
        );

        Ok(())
    }
}
