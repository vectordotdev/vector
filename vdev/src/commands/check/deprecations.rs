#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use anyhow::{Result, bail};
use semver::Version;

use crate::utils::{deprecation, git, paths};

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

        // Reject any fragment with a deprecated_since newer than the next minor release.
        match git::latest_release_version() {
            Ok(latest) => {
                let next_minor = Version::new(latest.major, latest.minor + 1, 0);
                let future: Vec<_> = entries
                    .iter()
                    .filter(|e| e.deprecated_since.0 > next_minor)
                    .collect();
                if !future.is_empty() {
                    for e in &future {
                        eprintln!(
                            "  future  {} (deprecated_since: {}, next release: {}.{})",
                            e.filename, e.deprecated_since, next_minor.major, next_minor.minor
                        );
                    }
                    bail!(
                        "{} fragment(s) have a deprecated_since version newer than the next release ({}.{}). \
                         Update deprecated_since to {} or earlier.",
                        future.len(),
                        next_minor.major,
                        next_minor.minor,
                        next_minor
                    );
                }
            }
            Err(e) => {
                bail!("could not determine latest release version: {e}");
            }
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

        let enacted_count = deprecation::validate_enacted(&repo_root)?;
        println!(
            "{} is up to date ({} enacted entries valid).",
            json_path.display(),
            enacted_count
        );

        Ok(())
    }
}
