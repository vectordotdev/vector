use std::{collections::BTreeSet, fmt::Write as _, path::Path, process::Command};

use anyhow::{Context, Result, anyhow, bail, ensure};
use glob::glob;
use semver::Version;
use serde::Deserialize;

use super::generate_cue::{HIGHLIGHTS_DIR, RELEASES_DIR, find_existing_upgrade_guide};
use crate::{
    app::CommandExt as _,
    utils::{cargo, paths},
};

const WEBSITE_URL: &str = "https://vector.dev";
const RELEASE_CALENDAR_URL: &str = "https://calendar.vector.dev";

/// Anchor of the release page's breaking-changes section.
const BREAKING_ANCHOR: &str = "breaking-changes";

/// Changelog groups in the order `website/layouts/releases/single.html` renders them, as
/// `(type, singular noun, plural noun, section anchor)`. `chore` is omitted: changelog
/// fragments only produce it for breaking changes, which get their own line.
const GROUPS: &[(&str, &str, &str, &str)] = &[
    (
        "security",
        "security change",
        "security changes",
        "security-changes",
    ),
    ("feat", "new feature", "new features", "new-features"),
    ("enhancement", "enhancement", "enhancements", "enhancements"),
    ("fix", "bug fix", "bug fixes", "bug-fixes"),
    ("deprecation", "deprecation", "deprecations", "deprecations"),
];

/// Uploads target/artifacts to GitHub releases
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Print the release description instead of creating the GitHub release
    #[arg(long)]
    dry_run: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = cargo::get_version()?;
        let parsed_version = Version::parse(&version)
            .with_context(|| format!("Invalid release version {version:?}"))?;
        let notes = release_notes(&paths::find_repo_root()?, &parsed_version)?;

        if self.dry_run {
            println!("{notes}");
            return Ok(());
        }

        let artifacts = glob("target/artifacts/*")
            .expect("failed to read glob pattern")
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("failed to read path: {e}"))?
            .into_iter()
            .map(|p| p.into_os_string().into_string())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("failed to turn path into string: {}", e.to_string_lossy()))?;

        let mut command = Command::new("gh");
        command.in_repo();
        command.args(
            [
                "release",
                "--repo",
                "vectordotdev/vector",
                "create",
                &format!("v{version}"),
                "--title",
                &format!("v{version}"),
                "--notes",
                &notes,
            ]
            .map(String::from)
            .into_iter()
            .chain(artifacts),
        );
        command.check_run()?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ChangelogEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    breaking: bool,
    #[serde(default)]
    contributors: Vec<String>,
}

/// Build the GitHub release description from the release CUE file and the upgrade guide.
fn release_notes(repo_root: &Path, version: &Version) -> Result<String> {
    let changelog = read_changelog(repo_root, version)?;
    let upgrade_guide = find_existing_upgrade_guide(&repo_root.join(HIGHLIGHTS_DIR), version)?;
    let upgrade_guide_slug = upgrade_guide
        .as_deref()
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| anyhow!("Bad upgrade guide filename: {}", path.display()))
        })
        .transpose()?;
    Ok(render_notes(version, &changelog, upgrade_guide_slug))
}

fn read_changelog(repo_root: &Path, version: &Version) -> Result<Vec<ChangelogEntry>> {
    let cue_path = repo_root.join(RELEASES_DIR).join(format!("{version}.cue"));
    ensure!(
        cue_path.is_file(),
        "{} does not exist. Prepare the release before publishing it.",
        cue_path.display()
    );

    let output = Command::new("cue")
        .arg("export")
        .arg(&cue_path)
        .args(["--out", "json", "-e"])
        .arg(format!("releases.\"{version}\".changelog"))
        .output()
        .context("Failed to run `cue export`")?;
    if !output.status.success() {
        bail!(
            "`cue export` of {} failed:\n{}",
            cue_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse the changelog in {}", cue_path.display()))
}

fn render_notes(
    version: &Version,
    changelog: &[ChangelogEntry],
    upgrade_guide_slug: Option<&str>,
) -> String {
    let release_url = format!("{WEBSITE_URL}/releases/{version}/");
    let upgrade_guide_url =
        upgrade_guide_slug.map(|slug| format!("{WEBSITE_URL}/highlights/{slug}/"));

    let mut out = format!(
        "The [COSE team](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team) is happy to announce Vector `{version}`! 🚀 Vector is a lightweight, ultra-fast tool for building observability pipelines that puts you in control of your data.\n\n"
    );

    if !changelog.is_empty() {
        let total = changelog.len();
        let contributors = changelog
            .iter()
            .flat_map(|entry| &entry.contributors)
            .collect::<BTreeSet<_>>()
            .len();
        match contributors {
            0 => {
                let noun = if total == 1 { "change" } else { "changes" };
                write!(out, "This release includes {total} {noun}:").unwrap();
            }
            1 => out.push_str("Thanks to the contributor who made "),
            _ => write!(out, "Thanks to the {contributors} contributors who made ").unwrap(),
        }
        if contributors > 0 {
            if total == 1 {
                out.push_str("the change");
            } else {
                write!(out, "the {total} changes").unwrap();
            }
            out.push_str(" in this release!");
        }
        out.push_str("\n\n");

        let breaking = changelog.iter().filter(|entry| entry.breaking).count();
        if breaking > 0 {
            let noun = if breaking == 1 {
                "breaking change"
            } else {
                "breaking changes"
            };
            // The release page renders its breaking-changes section only when the release
            // has an upgrade guide.
            let target = if upgrade_guide_url.is_some() {
                format!("{release_url}#{BREAKING_ANCHOR}")
            } else {
                release_url.clone()
            };
            writeln!(out, "- [{breaking} {noun}]({target})").unwrap();
        }

        for (kind, singular, plural, anchor) in GROUPS {
            let count = changelog.iter().filter(|entry| entry.kind == *kind).count();
            if count == 0 {
                continue;
            }
            let noun = if count == 1 { singular } else { plural };
            writeln!(out, "- [{count} {noun}]({release_url}#{anchor})").unwrap();
        }
        out.push('\n');
    }

    if let Some(url) = &upgrade_guide_url {
        writeln!(
            out,
            "Before upgrading, read the [{}.{} upgrade guide]({url}).\n",
            version.major, version.minor
        )
        .unwrap();
    }

    write!(
        out,
        "[View the full release notes]({release_url}) · [Release calendar]({RELEASE_CALENDAR_URL})"
    )
    .unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, breaking: bool, contributors: &[&str]) -> ChangelogEntry {
        ChangelogEntry {
            kind: kind.to_string(),
            breaking,
            contributors: contributors.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn thanks_unique_contributors_and_links_sections_and_upgrade_guide() {
        let changelog = [
            entry("chore", true, &["alice"]),
            entry("feat", false, &["bob", "alice"]),
            entry("fix", false, &["carol"]),
            entry("fix", false, &["bob"]),
        ];
        let notes = render_notes(
            &Version::new(0, 59, 0),
            &changelog,
            Some("2026-10-05-0-59-0-upgrade-guide"),
        );

        assert_eq!(
            notes,
            indoc::indoc! {"
                The [COSE team](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team) is happy to announce Vector `0.59.0`! 🚀 Vector is a lightweight, ultra-fast tool for building observability pipelines that puts you in control of your data.

                Thanks to the 3 contributors who made the 4 changes in this release!

                - [1 breaking change](https://vector.dev/releases/0.59.0/#breaking-changes)
                - [1 new feature](https://vector.dev/releases/0.59.0/#new-features)
                - [2 bug fixes](https://vector.dev/releases/0.59.0/#bug-fixes)

                Before upgrading, read the [0.59 upgrade guide](https://vector.dev/highlights/2026-10-05-0-59-0-upgrade-guide/).

                [View the full release notes](https://vector.dev/releases/0.59.0/) · [Release calendar](https://calendar.vector.dev)"}
        );
    }

    #[test]
    fn falls_back_without_contributors_or_upgrade_guide() {
        let changelog = [entry("chore", true, &[]), entry("fix", false, &[])];
        let notes = render_notes(&Version::new(0, 59, 1), &changelog, None);

        assert_eq!(
            notes,
            indoc::indoc! {"
                The [COSE team](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team) is happy to announce Vector `0.59.1`! 🚀 Vector is a lightweight, ultra-fast tool for building observability pipelines that puts you in control of your data.

                This release includes 2 changes:

                - [1 breaking change](https://vector.dev/releases/0.59.1/)
                - [1 bug fix](https://vector.dev/releases/0.59.1/#bug-fixes)

                [View the full release notes](https://vector.dev/releases/0.59.1/) · [Release calendar](https://calendar.vector.dev)"}
        );
    }
}
