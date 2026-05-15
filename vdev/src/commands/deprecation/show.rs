#![allow(clippy::print_stdout)]

use anyhow::Result;
use owo_colors::{OwoColorize, Stream::Stdout, Style};
use semver::Version;

use crate::utils::{
    deprecation::{self, DeprecationEntry, VersionOrTbd},
    git, paths,
};

/// Show upcoming and in-progress deprecation notices
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Filter to only show entries whose `deprecation_version` matches this release version.
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
            print_section_header(&format!("Deprecations enacted in {version}"));
            for e in &entries {
                print_entry(e, None);
            }
            return Ok(());
        }

        // Determine the next minor release version (best-effort).
        let next_minor: Option<Version> = git::latest_release_version()
            .ok()
            .map(|v| Version::new(v.major, v.minor + 1, 0));

        // Use the computed next minor as the "release" for partitioning.
        // If the next minor can't be determined, fall back to a sentinel version
        // that only `next` keywords will match (major=0, minor=0 never matches real versions).
        let partition_version = next_minor.clone().unwrap_or_else(|| Version::new(0, 0, 0));
        let p = deprecation::partition_by_release(entries, &partition_version);
        let enacted: Vec<&DeprecationEntry> = p.enacted.iter().collect();
        let announcing: Vec<&DeprecationEntry> = p.announcing.iter().collect();
        let preexisting: Vec<&DeprecationEntry> = p.planned.iter().collect();

        let next_label = match &next_minor {
            Some(v) => format!("{}.{}", v.major, v.minor),
            None => "next".to_string(),
        };

        let nm = next_minor.as_ref();
        print_section(
            &format!("Enacted in next release ({next_label})"),
            &enacted,
            nm,
        );
        print_section(
            &format!("Announced in next release ({next_label})"),
            &announcing,
            nm,
        );
        print_section("Pre-existing deprecations", &preexisting, nm);

        Ok(())
    }
}

fn print_section_header(title: &str) {
    let style = Style::new().bold().underline();
    println!("{}", title.if_supports_color(Stdout, |t| t.style(style)));
    println!();
}

fn print_section(title: &str, entries: &[&DeprecationEntry], next_minor: Option<&Version>) {
    print_section_header(title);
    if entries.is_empty() {
        println!("{}", "(none)".if_supports_color(Stdout, |t| t.dimmed()));
    } else {
        for e in entries {
            print_entry(e, next_minor);
        }
    }
    println!();
}

fn print_entry(e: &DeprecationEntry, next_minor: Option<&Version>) {
    println!("{}", e.what.if_supports_color(Stdout, |t| t.bold()));
    println!(
        "  {} {}",
        "announced: ".if_supports_color(Stdout, |t| t.dimmed()),
        format_version(&e.announcement_version, next_minor),
    );
    println!(
        "  {} {}",
        "deprecated:".if_supports_color(Stdout, |t| t.dimmed()),
        format_version(&e.deprecation_version, next_minor),
    );
    if !e.description.is_empty() {
        println!();
        for line in e.description.lines() {
            println!("  {}", line.if_supports_color(Stdout, |t| t.italic()));
        }
    }
    println!();
}

fn format_version(v: &VersionOrTbd, next_minor: Option<&Version>) -> String {
    let is_next = matches!(v, VersionOrTbd::Next)
        || matches!((v, next_minor), (VersionOrTbd::Version(_), Some(nv)) if v.matches_release(nv));

    if is_next {
        let style = Style::new().bright_red().bold();
        return v
            .to_string()
            .if_supports_color(Stdout, |t| t.style(style))
            .to_string();
    }

    match v {
        VersionOrTbd::Tbd => "TBD"
            .if_supports_color(Stdout, |t| t.bright_yellow())
            .to_string(),
        _ => v
            .to_string()
            .if_supports_color(Stdout, |t| t.bright_cyan())
            .to_string(),
    }
}
