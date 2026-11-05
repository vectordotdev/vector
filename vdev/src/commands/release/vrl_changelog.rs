//! Import VRL's generated breaking entries without interpreting authored migration text.

use std::{collections::HashSet, fmt::Write as _, process::Command, sync::LazyLock};

use anyhow::{Context, Result, ensure};
use regex::Regex;
use semver::Version;

static ENTRY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^- <a id="(v[0-9]+\.[0-9]+\.[0-9]+-[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)"></a> \*\*(.+)\*\*\r?$"#)
        .expect("valid generated VRL entry regex")
});
static BOUNDARY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(?:- |#{1,3} )").expect("valid VRL boundary regex"));

pub(super) struct Changelog {
    pub notes: String,
    pub migrations: Vec<Migration>,
}

pub(super) struct Migration {
    pub title: String,
    pub anchor: String,
    pub body: String,
}

pub(super) fn fetch(version: &Version) -> Result<String> {
    super::ensure_stable(version, "VRL version")?;
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/vectordotdev/vrl/contents/CHANGELOG.md?ref=v{version}"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
        .context("Failed to fetch VRL CHANGELOG.md")?;
    ensure!(
        output.status.success(),
        "gh api CHANGELOG.md failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    release_section(
        &String::from_utf8(output.stdout).context("VRL changelog is not UTF-8")?,
        version,
    )
}

fn release_section(changelog: &str, version: &Version) -> Result<String> {
    let prefix = format!("## [{version} (");
    let mut lines = changelog
        .lines()
        .skip_while(|line| !line.starts_with(&prefix));
    let first = lines
        .next()
        .with_context(|| format!("No release section for VRL {version}"))?;
    Ok(std::iter::once(first)
        .chain(lines.take_while(|line| !line.starts_with("## [")))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(super) fn extract(notes: &str, vector_version: &Version) -> Result<Changelog> {
    let mut output = String::new();
    let mut migrations = Vec::new();
    let mut anchors = HashSet::new();
    let mut cursor = 0;
    for entry in ENTRY.captures_iter(notes) {
        let header = entry.get(0).unwrap();
        // VRL indents every authored continuation line by two spaces. Only the next
        // generated list item or category heading can terminate this entry.
        let end = BOUNDARY
            .find(notes.get(header.end()..).unwrap())
            .map_or(notes.len(), |next| header.end() + next.start());
        let body = notes
            .get(header.end()..end)
            .unwrap()
            .lines()
            .map(|line| line.strip_prefix("  ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        let (body, credit) = body
            .trim_matches('\n')
            .rsplit_once("\n\n")
            .context("Generated VRL breaking entry has no attribution")?;
        ensure!(
            credit.starts_with("*Thanks to ") && credit.ends_with("!*"),
            "Unrecognized VRL attribution: {credit}"
        );
        let anchor = entry[1].to_owned();
        ensure!(
            anchors.insert(anchor.clone()),
            "Duplicate VRL migration anchor: {anchor}"
        );
        let title = entry[2].to_owned();
        output.push_str(notes.get(cursor..header.start()).unwrap());
        write!(output, "- **{title}**\n\n  Refer to the [upgrade guide](/releases/{vector_version}/#{anchor}) for migration instructions.\n\n  {credit}\n\n").unwrap();
        migrations.push(Migration {
            title,
            anchor,
            body: format!("{body}\n\n{credit}"),
        });
        cursor = end;
    }
    output.push_str(notes.get(cursor..).unwrap());
    Ok(Changelog {
        notes: output,
        migrations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(migration: &str) -> String {
        let body = format!("**Summary**\n\nBehavior changed.\n\n**Migration**\n\n{migration}");
        let indented = body
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("  {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "## [0.39.0 (2026-11-03)](tag)\n\n### Breaking Changes & Upgrade Guide\n\n- <a id=\"v0.39.0-a-change\"></a> **A change**\n\n{indented}\n\n  *Thanks to [alice](https://github.com/alice) for contributing PR [#1](https://github.com/vectordotdev/vrl/pull/1)!*\n\n### Fixes\n\n- A normal fix.\n"
        )
    }

    #[test]
    fn preserves_migrations_without_requiring_examples() {
        for migration in ["Use the replacement.", "N/A", ""] {
            let imported = extract(&notes(migration), &Version::new(0, 62, 0)).unwrap();
            assert_eq!(imported.migrations.len(), 1);
            assert!(imported.migrations[0].body.contains(migration));
            assert!(
                imported.migrations[0]
                    .body
                    .starts_with("**Summary**\n\nBehavior changed.")
            );
            assert!(
                imported
                    .notes
                    .contains("/releases/0.62.0/#v0.39.0-a-change")
            );
            assert!(imported.notes.ends_with("### Fixes\n\n- A normal fix.\n"));
            assert!(imported.migrations[0].body.contains("Thanks to [alice]"));
        }
    }

    #[test]
    fn preserves_authored_markdown_including_marker_lookalikes() {
        let migration = "- Keep café in this list.\n  - And this nested item.\n\n#### Old\n\n```markdown\n- <a id=\"v0.39.0-fake\"></a> **Fake**\n## [0.1.0 (date)](tag)\n**Migration**\n```\n\n    .indented = true  ";
        let source = notes(migration).replace("v0.39.0-a-change", "v0.39.0-a--change");
        let imported = extract(&source, &Version::new(0, 62, 0)).unwrap();
        assert_eq!(imported.migrations.len(), 1);
        assert_eq!(imported.migrations[0].anchor, "v0.39.0-a--change");
        assert!(imported.migrations[0].body.contains(migration));
    }

    #[test]
    fn leaves_older_unstructured_entries_unchanged() {
        let source = "## [0.38.0 (date)](tag)\n\n### Breaking Changes & Upgrade Guide\n\n- Older entry.\n\n  **Before**\n\n  ```vrl\n  old!()\n  ```\n";
        let imported = extract(source, &Version::new(0, 62, 0)).unwrap();
        assert!(imported.migrations.is_empty());
        assert_eq!(imported.notes, source);
    }

    #[test]
    fn rejects_duplicate_entry_anchors() {
        let source = notes("Use the replacement.");
        assert!(extract(&format!("{source}\n{source}"), &Version::new(0, 62, 0)).is_err());
    }

    #[test]
    fn selects_the_requested_release() {
        let source = format!(
            "## [0.40.0 (date)](tag)\n\n{0}\n## [0.38.0 (date)](tag)\n",
            notes("Use the replacement.")
        );
        let selected = release_section(&source, &Version::new(0, 39, 0)).unwrap();
        assert!(selected.starts_with("## [0.39.0 ("));
        assert!(selected.contains("  **Summary**"));
        assert!(!selected.contains("## [0.38.0"));
        assert!(release_section(&source, &Version::new(0, 1, 0)).is_err());
    }
}
