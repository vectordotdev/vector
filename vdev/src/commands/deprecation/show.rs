#![allow(clippy::print_stdout)]

use anyhow::Result;
use semver::Version;

use crate::utils::{
    deprecation::{self, DeprecationEntry, VersionOrTbd},
    git, paths,
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

        entries.sort_by(|a, b| a.deprecation_version.cmp(&b.deprecation_version));

        // When --version is given, show only entries enacted in that release.
        if let Some(ref version) = self.version {
            entries.retain(|e| e.deprecation_version.matches_release(version));
            if entries.is_empty() {
                println!("No deprecations enacted in v{version}.");
                return Ok(());
            }
            println!("Deprecations enacted in {version}:");
            println!();
            for e in &entries {
                print_entry(e);
            }
            return Ok(());
        }

        // Determine the next minor release version (best-effort).
        let next_minor: Option<Version> = git::latest_release_version()
            .ok()
            .map(|v| Version::new(v.major, v.minor + 1, 0));

        // Group entries into three buckets:
        //   1. Enacted in next release  — deprecation_version is `next` or matches the next minor
        //   2. Announced in next release — announcement_version is `next` or matches next minor,
        //                                  but NOT in bucket 1
        //   3. Pre-existing             — everything else
        let is_next_release = |v: &VersionOrTbd| match next_minor.as_ref() {
            Some(nv) => v.matches_release(nv),
            None => matches!(v, VersionOrTbd::Next),
        };

        let mut enacted: Vec<&DeprecationEntry> = Vec::new();
        let mut announcing: Vec<&DeprecationEntry> = Vec::new();
        let mut preexisting: Vec<&DeprecationEntry> = Vec::new();

        for e in &entries {
            if is_next_release(&e.deprecation_version) {
                enacted.push(e);
            } else if is_next_release(&e.announcement_version) {
                announcing.push(e);
            } else {
                preexisting.push(e);
            }
        }

        let next_label = match &next_minor {
            Some(v) => format!("{}.{}", v.major, v.minor),
            None => "next".to_string(),
        };

        print_section(
            &format!("Enacted in next release ({next_label})"),
            &enacted,
        );
        print_section(
            &format!("Announced in next release ({next_label})"),
            &announcing,
        );
        print_section("Pre-existing deprecations", &preexisting);

        Ok(())
    }
}

fn print_section(title: &str, entries: &[&DeprecationEntry]) {
    println!("{title}:");
    println!();
    if entries.is_empty() {
        println!("(none)");
    } else {
        for e in entries {
            print_entry(e);
        }
    }
    println!();
}

fn print_entry(e: &DeprecationEntry) {
    println!("{}", e.what);
    println!("  announced:  {}", e.announcement_version);
    println!("  deprecated: {}", e.deprecation_version);
    if !e.description.is_empty() {
        println!();
        for line in e.description.lines() {
            println!("  {line}");
        }
    }
    println!();
}
