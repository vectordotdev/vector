use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use glob::glob;
use semver::Version;
use serde::{Deserialize, Serialize};

use super::generate_cue::{HIGHLIGHTS_DIR, RELEASES_DIR, find_existing_upgrade_guide};
use crate::{
    app::CommandExt as _,
    utils::{cargo, paths},
};

const WEBSITE_URL: &str = "https://vector.dev";

/// Anchor of the release page's breaking-changes section.
const BREAKING_ANCHOR: &str = "breaking-changes";

/// Release notes template, relative to the repository root. It is read at runtime, not
/// embedded, so that wording can be changed without recompiling `vdev`.
const RELEASE_NOTES_TEMPLATE_PATH: &str = "vdev/src/commands/release/release_notes.hbs";

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

    /// Path to the release notes Handlebars template. Relative paths resolve against the
    /// repository root. Defaults to `vdev/src/commands/release/release_notes.hbs`.
    #[arg(long, env = "VDEV_RELEASE_NOTES_TEMPLATE")]
    template: Option<PathBuf>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let version = cargo::get_version()?;
        let parsed_version = Version::parse(&version)
            .with_context(|| format!("Invalid release version {version:?}"))?;
        let notes = release_notes(&repo_root, &parsed_version, self.template.as_deref())?;

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
fn release_notes(
    repo_root: &Path,
    version: &Version,
    template_path: Option<&Path>,
) -> Result<String> {
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
    let template = load_template(repo_root, template_path)?;
    render_notes(version, &changelog, upgrade_guide_slug, &template)
}

/// Read the release notes template, defaulting to [`RELEASE_NOTES_TEMPLATE_PATH`] under the
/// repository root. A relative `template_path` is resolved against the repository root too.
fn load_template(repo_root: &Path, template_path: Option<&Path>) -> Result<String> {
    let path = match template_path {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => repo_root.join(path),
        None => repo_root.join(RELEASE_NOTES_TEMPLATE_PATH),
    };
    fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))
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

/// Data injected into the release notes template (`release_notes.hbs`).
#[derive(Serialize)]
struct ReleaseNotes {
    version: String,
    major: u64,
    minor: u64,
    contributors: String,
    changes: usize,
    one_change: bool,
    breaking: Option<Section>,
    sections: Vec<Section>,
    upgrade_guide_url: Option<String>,
    release_url: String,
}

/// A single `- [count noun](url)` line in the release notes.
#[derive(Serialize)]
struct Section {
    count: usize,
    noun: &'static str,
    url: String,
}

fn render_notes(
    version: &Version,
    changelog: &[ChangelogEntry],
    upgrade_guide_slug: Option<&str>,
    template: &str,
) -> Result<String> {
    let release_url = format!("{WEBSITE_URL}/releases/{version}/");
    let upgrade_guide_url =
        upgrade_guide_slug.map(|slug| format!("{WEBSITE_URL}/highlights/{slug}/"));

    let changes = changelog.len();
    let contributors = match changelog
        .iter()
        .flat_map(|entry| &entry.contributors)
        .collect::<BTreeSet<_>>()
        .len()
    {
        0 => String::new(),
        1 => "contributor".to_string(),
        count => format!("{count} contributors"),
    };

    let breaking = changelog.iter().filter(|entry| entry.breaking).count();
    // The release page renders its breaking-changes section only when the release has an
    // upgrade guide.
    let breaking = (breaking > 0).then(|| Section {
        count: breaking,
        noun: if breaking == 1 {
            "breaking change"
        } else {
            "breaking changes"
        },
        url: if upgrade_guide_url.is_some() {
            format!("{release_url}#{BREAKING_ANCHOR}")
        } else {
            release_url.clone()
        },
    });

    let sections = GROUPS
        .iter()
        .filter_map(|(kind, singular, plural, anchor)| {
            let count = changelog.iter().filter(|entry| entry.kind == *kind).count();
            (count > 0).then(|| Section {
                count,
                noun: if count == 1 { singular } else { plural },
                url: format!("{release_url}#{anchor}"),
            })
        })
        .collect();

    let data = ReleaseNotes {
        version: version.to_string(),
        major: version.major,
        minor: version.minor,
        contributors,
        changes,
        one_change: changes == 1,
        breaking,
        sections,
        upgrade_guide_url,
        release_url,
    };

    let mut handlebars = handlebars::Handlebars::new();
    // The notes are Markdown, not HTML: never escape the injected values.
    handlebars.register_escape_fn(handlebars::no_escape);
    handlebars.register_template_string("release_notes", template)?;
    // The template file ends with a newline (editorconfig); the release body must not.
    Ok(handlebars
        .render("release_notes", &data)?
        .trim_end()
        .to_string())
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

    /// Exposes every injected field. Tests use it instead of `release_notes.hbs` so that
    /// changing the prose there does not require touching this file.
    const FIXTURE: &str = concat!(
        "{{version}}|{{major}}.{{minor}}|{{contributors}}|{{changes}}|{{one_change}}|",
        "{{#if breaking}}breaking:{{breaking.count}}/{{breaking.noun}}/{{breaking.url}}|{{/if}}",
        "{{#each sections}}section:{{count}}/{{noun}}/{{url}}|{{/each}}",
        "{{#if upgrade_guide_url}}upgrade:{{upgrade_guide_url}}|{{/if}}",
        "release:{{release_url}}",
    );

    fn render(version: &Version, changelog: &[ChangelogEntry], slug: Option<&str>) -> String {
        render_notes(version, changelog, slug, FIXTURE).unwrap()
    }

    #[test]
    fn resolves_template_override_absolute_and_relative() {
        let dir = tempfile::tempdir().unwrap();

        let absolute = dir.path().join("notes.hbs");
        fs::write(&absolute, "absolute").unwrap();
        assert_eq!(
            load_template(Path::new("/unused"), Some(&absolute)).unwrap(),
            "absolute"
        );

        fs::write(dir.path().join("relative.hbs"), "relative").unwrap();
        assert_eq!(
            load_template(dir.path(), Some(Path::new("relative.hbs"))).unwrap(),
            "relative"
        );
    }

    #[test]
    fn injects_counts_plural_nouns_and_section_links() {
        let changelog = [
            entry("chore", true, &["alice"]),
            entry("feat", false, &["bob", "alice"]),
            entry("fix", false, &["carol"]),
            entry("fix", false, &["bob"]),
        ];
        let notes = render(
            &Version::new(0, 59, 0),
            &changelog,
            Some("2026-10-05-0-59-0-upgrade-guide"),
        );

        assert_eq!(
            notes,
            "0.59.0|0.59|3 contributors|4|false|\
             breaking:1/breaking change/https://vector.dev/releases/0.59.0/#breaking-changes|\
             section:1/new feature/https://vector.dev/releases/0.59.0/#new-features|\
             section:2/bug fixes/https://vector.dev/releases/0.59.0/#bug-fixes|\
             upgrade:https://vector.dev/highlights/2026-10-05-0-59-0-upgrade-guide/|\
             release:https://vector.dev/releases/0.59.0/"
        );
    }

    #[test]
    fn omits_contributor_summary_and_upgrade_guide_when_absent() {
        let changelog = [entry("chore", true, &[]), entry("fix", false, &[])];
        let notes = render(&Version::new(0, 59, 1), &changelog, None);

        assert_eq!(
            notes,
            "0.59.1|0.59||2|false|\
             breaking:1/breaking change/https://vector.dev/releases/0.59.1/|\
             section:1/bug fix/https://vector.dev/releases/0.59.1/#bug-fixes|\
             release:https://vector.dev/releases/0.59.1/"
        );
    }

    #[test]
    fn flags_single_contributor_and_single_change() {
        let changelog = [entry("fix", false, &["alice"])];
        let notes = render(&Version::new(0, 60, 0), &changelog, None);

        assert_eq!(
            notes,
            "0.60.0|0.60|contributor|1|true|\
             section:1/bug fix/https://vector.dev/releases/0.60.0/#bug-fixes|\
             release:https://vector.dev/releases/0.60.0/"
        );
    }

    #[test]
    fn pluralizes_multiple_breaking_changes_without_sections() {
        let changelog = [
            entry("chore", true, &["alice"]),
            entry("chore", true, &["alice"]),
        ];
        let notes = render(&Version::new(0, 60, 0), &changelog, None);

        assert_eq!(
            notes,
            "0.60.0|0.60|contributor|2|false|\
             breaking:2/breaking changes/https://vector.dev/releases/0.60.0/|\
             release:https://vector.dev/releases/0.60.0/"
        );
    }

    #[test]
    fn production_template_loads_and_references_every_link() -> Result<()> {
        let changelog = [
            entry("chore", true, &["alice"]),
            entry("feat", false, &["bob", "alice"]),
            entry("fix", false, &["carol"]),
            entry("fix", false, &["bob"]),
        ];
        let template = load_template(&paths::find_repo_root()?, None)?;
        let notes = render_notes(
            &Version::new(0, 59, 0),
            &changelog,
            Some("2026-10-05-0-59-0-upgrade-guide"),
            &template,
        )?;

        for expected in [
            "https://vector.dev/releases/0.59.0/#breaking-changes",
            "https://vector.dev/releases/0.59.0/#new-features",
            "https://vector.dev/releases/0.59.0/#bug-fixes",
            "https://vector.dev/highlights/2026-10-05-0-59-0-upgrade-guide/",
            "https://vector.dev/releases/0.59.0/",
            "https://calendar.vector.dev",
        ] {
            assert!(
                notes.contains(expected),
                "missing {expected:?} in:\n{notes}"
            );
        }

        Ok(())
    }
}
