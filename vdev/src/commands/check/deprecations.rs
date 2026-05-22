#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};

use crate::utils::{deprecation, git, paths};

/// Check that all deprecation.d fragments are valid
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
            return Ok(());
        }

        for entry in &entries {
            println!("  ok  {}", entry.filename);
        }
        println!("{} deprecation fragment(s) are valid.", entries.len());

        // Validate that no fragment's deprecation_version has already been released.
        match git::latest_release_version() {
            Ok(latest) => {
                let mut stale = false;
                for entry in &entries {
                    if !entry.deprecation_version.is_future_relative_to(&latest) {
                        eprintln!(
                            "  STALE  {} (deprecation_version {} is not greater than latest release {})",
                            entry.filename, entry.deprecation_version, latest
                        );
                        stale = true;
                    }
                }
                if stale {
                    bail!(
                        "One or more deprecation fragments have a deprecation_version \
                         that is not greater than the latest release v{latest}. \
                         These should have been enacted and removed during the {latest} release."
                    );
                }
                println!(
                    "All deprecation_versions are greater than latest release v{latest}."
                );
            }
            Err(e) => {
                eprintln!("Warning: could not determine latest release version ({e}); skipping version freshness check.");
            }
        }

        Ok(())
    }
}
