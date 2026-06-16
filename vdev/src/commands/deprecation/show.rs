#![allow(clippy::print_stdout)]

use anyhow::Result;
use owo_colors::{OwoColorize, Stream::Stdout, Style};
use semver::Version;

use crate::utils::{
    deprecation::{self, DeprecationEntry},
    git, paths,
};

/// Show current and upcoming deprecation notices
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// The next release version to use for partitioning (e.g. "0.56.0").
    /// Defaults to the next minor after the latest git tag.
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
        entries.sort_by(|a, b| a.deprecated_since.cmp(&b.deprecated_since));

        // Determine the target release version (best-effort).
        let next_minor: Option<Version> = if let Some(v) = self.version {
            Some(v)
        } else {
            git::latest_release_version()
                .ok()
                .map(|v| Version::new(v.major, v.minor + 1, 0))
        };

        let partition_version = next_minor.clone().unwrap_or_else(|| Version::new(0, 0, 0));
        let p = deprecation::partition_by_release(entries, &partition_version);
        let announcing: Vec<&DeprecationEntry> = p.announcing.iter().collect();
        let planned: Vec<&DeprecationEntry> = p.planned.iter().collect();
        let future: Vec<&DeprecationEntry> = p.future.iter().collect();

        let next_label = match &next_minor {
            Some(v) => format!("{}.{}", v.major, v.minor),
            None => "next".to_string(),
        };

        let mut removing: Vec<deprecation::EnactedEntry> = Vec::new();
        if let Some(ref v) = next_minor {
            let enacted = deprecation::read_enacted(&repo_root)?;
            removing = enacted
                .into_iter()
                .filter(|e| {
                    Version::parse(&e.removed_in)
                        .ok()
                        .is_some_and(|rv| rv.major == v.major && rv.minor == v.minor)
                })
                .collect();
        }

        if announcing.is_empty() && planned.is_empty() && future.is_empty() && removing.is_empty() {
            println!("No deprecation notices found.");
            return Ok(());
        }

        if next_minor.is_some() {
            let removing_refs: Vec<&deprecation::EnactedEntry> = removing.iter().collect();
            print_enacted_section(&next_label, &removing_refs);
        }

        print_announcing_section(&next_label, &announcing);
        print_section("Previously announced", &planned);
        if !future.is_empty() {
            print_section("Announced for a future release", &future);
        }

        Ok(())
    }
}

fn print_announcing_section(next_label: &str, entries: &[&DeprecationEntry]) {
    let style = Style::new().bold().underline();
    let title = format!("Announced in next release ({next_label})");
    println!("{}", title.if_supports_color(Stdout, |t| t.style(style)));
    println!();
    if entries.is_empty() {
        println!("{}", "(none)".if_supports_color(Stdout, |t| t.dimmed()));
    } else {
        for e in entries {
            print_entry(e);
        }
    }
    println!();
}

fn print_section(title: &str, entries: &[&DeprecationEntry]) {
    let style = Style::new().bold().underline();
    println!("{}", title.if_supports_color(Stdout, |t| t.style(style)));
    println!();
    if entries.is_empty() {
        println!("{}", "(none)".if_supports_color(Stdout, |t| t.dimmed()));
    } else {
        for e in entries {
            print_entry(e);
        }
    }
    println!();
}

fn print_enacted_section(next_label: &str, entries: &[&deprecation::EnactedEntry]) {
    let style = Style::new().bold().underline();
    let title = format!("Removed in {next_label}");
    println!("{}", title.if_supports_color(Stdout, |t| t.style(style)));
    println!();
    if entries.is_empty() {
        println!("{}", "(none)".if_supports_color(Stdout, |t| t.dimmed()));
    } else {
        for e in entries {
            println!("{}", e.what.if_supports_color(Stdout, |t| t.bold()));
            println!(
                "  {} {}",
                "deprecated_since:".if_supports_color(Stdout, |t| t.dimmed()),
                e.deprecated_since
                    .if_supports_color(Stdout, |t| t.bright_cyan()),
            );
            if !e.description.is_empty() {
                println!();
                for line in e.description.lines() {
                    println!("  {}", line.if_supports_color(Stdout, |t| t.italic()));
                }
            }
            println!();
        }
    }
    println!();
}

fn print_entry(e: &DeprecationEntry) {
    println!("{}", e.what.if_supports_color(Stdout, |t| t.bold()));
    println!(
        "  {} {}",
        "deprecated_since:".if_supports_color(Stdout, |t| t.dimmed()),
        e.deprecated_since
            .to_string()
            .if_supports_color(Stdout, |t| t.bright_cyan()),
    );
    if !e.description.is_empty() {
        println!();
        for line in e.description.lines() {
            println!("  {}", line.if_supports_color(Stdout, |t| t.italic()));
        }
    }
    println!();
}
