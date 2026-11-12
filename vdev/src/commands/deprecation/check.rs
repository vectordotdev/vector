#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use anyhow::{Result, bail};
use semver::Version;

use crate::utils::{deprecation, git, paths};

/// Check that the active (deprecation.d) and enacted deprecation notices are valid
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let active_dir = repo_root.join(deprecation::DEPRECATION_DIR);
        let enacted_dir = repo_root.join(deprecation::ENACTED_DIR);
        if !active_dir.is_dir() && !enacted_dir.is_dir() {
            bail!(
                "Neither {} nor {} found; the deprecation fragment system is not installed in this repo.",
                active_dir.display(),
                enacted_dir.display()
            );
        }

        let deprecations = deprecation::read_all(&repo_root)?;
        for entry in &deprecations.active {
            println!("  ok  {}/{}", deprecation::DEPRECATION_DIR, entry.filename);
        }
        for entry in &deprecations.enacted {
            println!("  ok  {}/{}", deprecation::ENACTED_DIR, entry.filename);
        }
        println!(
            "{} active and {} enacted deprecation notice(s) are valid.",
            deprecations.active.len(),
            deprecations.enacted.len()
        );

        // Reject any fragment with a deprecated_since newer than the next
        // minor release. Skipped (with a warning) when the checkout has no
        // release tags so shallow CI/source checkouts can still validate
        // fragment frontmatter.
        match git::latest_release_version() {
            Ok(latest) => {
                let next_minor = Version::new(latest.major, latest.minor + 1, 0);
                let future: Vec<_> = deprecations
                    .active
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

        Ok(())
    }
}
