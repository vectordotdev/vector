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

        if entries.is_empty() {
            println!("No deprecation notices found.");
            return Ok(());
        }

        entries.sort_by(|a, b| a.deprecated_since.cmp(&b.deprecated_since));

        // Determine the next minor release version (best-effort).
        let explicit_version = self.version.is_some();
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

        let next_label = match &next_minor {
            Some(v) => format!("{}.{}", v.major, v.minor),
            None => "next".to_string(),
        };

        print_announcing_section(&next_label, &announcing);
        if !explicit_version {
            print_section("Previously announced", &planned);
        }

        Ok(())
    }
}

/// Print the "Announced in next release" section header with the version label in bold red.
fn print_announcing_section(next_label: &str, entries: &[&DeprecationEntry]) {
    let header_style = Style::new().underline().white();
    let title = format!("Announced in next release ({next_label})");
    println!(
        "{}",
        title.if_supports_color(Stdout, |t| t.style(header_style))
    );
    println!();
    if entries.is_empty() {
        println!("{}", "(none)".if_supports_color(Stdout, |t| t.dimmed()));
    } else {
        for e in entries {
            print_entry(e, true);
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
            print_entry(e, false);
        }
    }
    println!();
}

fn print_entry(e: &DeprecationEntry, highlight_version: bool) {
    println!("{}", e.what.if_supports_color(Stdout, |t| t.bold()));
    let version_str = e.deprecated_since.to_string();
    let version_colored = if highlight_version {
        owo_colors::OwoColorize::if_supports_color(&version_str, Stdout, |t| {
            t.style(Style::new().bold().red())
        })
        .to_string()
    } else {
        owo_colors::OwoColorize::if_supports_color(&version_str, Stdout, |t| t.bright_cyan())
            .to_string()
    };
    println!(
        "  {} {}",
        "deprecated_since:".if_supports_color(Stdout, |t| t.dimmed()),
        version_colored,
    );
    if !e.description.is_empty() {
        println!();
        for line in e.description.lines() {
            println!("  {}", line.if_supports_color(Stdout, |t| t.italic()));
        }
    }
    println!();
}
