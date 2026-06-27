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

        let json_path = repo_root.join(deprecation::DEPRECATIONS_JSON);
        let entries = if dir.is_dir() {
            deprecation::read_deprecation_fragments(&dir)?
        } else if json_path.is_file() {
            println!(
                "{} not found; validating generated JSON only.",
                dir.display()
            );
            Vec::new()
        } else {
            bail!(
                "Neither {} nor {} found; the deprecation fragment system is not installed in this repo.",
                dir.display(),
                json_path.display()
            );
        };

        if entries.is_empty() {
            println!("No deprecation fragments found.");
        } else {
            for entry in &entries {
                println!("  ok  {}", entry.filename);
            }
            println!("{} deprecation fragment(s) are valid.", entries.len());
        }

        // Reject any fragment with a deprecated_since newer than the next
        // minor release. Skipped (with a warning) when the checkout has no
        // release tags so shallow CI/source checkouts can still validate
        // fragment frontmatter + generated JSON.
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
                eprintln!(
                    "Warning: skipping future-version validation; could not determine latest release version: {e}"
                );
            }
        }

        let on_disk = std::fs::read_to_string(&json_path).unwrap_or_default();
        let expected = deprecation::rendered_json(&repo_root)?;
        if on_disk != expected {
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
