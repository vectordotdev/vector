#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};

use crate::utils::{deprecation, paths};

/// Check deprecation.d fragments are valid and that generated/deprecations.json is up to date
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

        let json_path = repo_root.join(deprecation::DEPRECATIONS_JSON);
        let before = std::fs::read_to_string(&json_path).unwrap_or_default();

        deprecation::sync_deprecations_cue(&repo_root)?;

        let after = std::fs::read_to_string(&json_path)?;
        if before != after {
            bail!(
                "{} is out of date. Run `cargo vdev deprecation generate` and commit the result.",
                json_path.display()
            );
        }

        println!("{} is up to date.", json_path.display());

        Ok(())
    }
}
