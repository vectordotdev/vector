#![allow(clippy::print_stdout)]

use anyhow::{Result, bail};

use crate::utils::{deprecation, paths};

/// Check that all deprecation.d fragments are valid and deprecations.json is in sync
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

        // Verify generated/deprecations.json is in sync with deprecation.d/.
        let json_path = repo_root.join(deprecation::DEPRECATIONS_JSON);
        if json_path.exists() {
            let enacted = deprecation::read_enacted(&repo_root)?;
            let expected = deprecation::render_deprecations_cue_for_check(&entries, &enacted);
            let actual = std::fs::read_to_string(&json_path)?;
            if actual != expected {
                bail!(
                    "{} is out of sync with deprecation.d/. Run `cargo vdev deprecation sync-cue` to regenerate it.",
                    json_path.display()
                );
            }
            println!("{} is up to date.", json_path.display());
        }

        Ok(())
    }
}
