//! Import VRL release notes and extract their migration instructions before publishing.

use std::{collections::HashMap, ops::Range, process::Command, sync::LazyLock};

use anyhow::{Context, Result, bail};
use pulldown_cmark::{Event, Parser, Tag};
use regex::Regex;
use semver::Version;

use super::generate_cue::BreakingDetails;

static PULL_REQUEST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https://github\.com/vectordotdev/vrl/pull/([0-9]+)")
        .expect("valid VRL pull-request regex")
});

static STRUCTURED_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^<a id="([^"]+)"></a> \*\*(.+)\*\*$"#).expect("valid structured VRL title regex")
});

pub(super) struct Changelog {
    pub notes: String,
    pub migrations: Vec<BreakingDetails>,
}

pub(super) fn fetch(version: &Version) -> Result<String> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/vectordotdev/vrl/contents/CHANGELOG.md?ref=v{version}"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
        .context("Failed to run `gh api` for VRL CHANGELOG.md")?;
    if !output.status.success() {
        bail!(
            "gh api CHANGELOG.md failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let changelog = String::from_utf8(output.stdout).context("CHANGELOG.md is not valid UTF-8")?;
    release_section(&changelog, version)
}

fn release_section(changelog: &str, version: &Version) -> Result<String> {
    let prefix = format!("## [{version} ");
    let mut section = Vec::new();
    for (line, code) in markdown_lines(changelog) {
        if !code && line.starts_with("## ") {
            if !section.is_empty() {
                break;
            }
            if !line.starts_with(&prefix) {
                continue;
            }
        }
        if !section.is_empty() || (!code && line.starts_with(&prefix)) {
            section.push(line);
        }
    }
    if section.is_empty() {
        bail!("No release section for VRL {version} found in CHANGELOG.md");
    }
    Ok(section.join("\n"))
}

/// Extract entries under Breaking Changes and entries with explicit migration notes
/// (Migration/Upgrade Guide, Before/After, or Old/New). Never infer impact from prose.
pub(super) fn extract(notes: &str, vector_version: &Version) -> Changelog {
    let lines: Vec<_> = notes.lines().collect();
    let mut ranges = Vec::new();
    let mut current = None;
    let mut breaking = false;
    let anchor_prefix = notes.lines().find_map(|line| {
        let version = line.strip_prefix("## [")?.split_once(' ')?.0;
        version
            .parse::<Version>()
            .ok()
            .map(|version| format!("v{version}-"))
    });
    for (index, (line, code)) in markdown_lines(notes).enumerate() {
        if code {
            continue;
        }
        let migration_heading =
            current.is_some() && line.starts_with("### ") && is_migration_heading(line);
        if line.starts_with("- ")
            || line.starts_with("## ")
            || (line.starts_with("### ") && !migration_heading)
        {
            if let Some((start, is_breaking)) = current.take() {
                ranges.push((start..index, is_breaking));
            }
            if let Some(heading) = line.strip_prefix("### ") {
                breaking = heading.to_ascii_lowercase().starts_with("breaking changes");
            } else if line.starts_with("## ") {
                breaking = false;
            }
            if line.starts_with("- ") {
                current = Some((index, breaking));
            }
        }
    }
    if let Some((start, is_breaking)) = current {
        ranges.push((start..lines.len(), is_breaking));
    }

    let mut migrations = Vec::new();
    let mut anchors = HashMap::new();
    let mut output = Vec::new();
    let mut cursor = 0;
    for (range, breaking) in ranges {
        if let Some((mut migration, credit)) =
            migration(&lines, &range, breaking, anchor_prefix.as_deref())
        {
            let count = anchors.entry(migration.anchor.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                migration.anchor = format!("{}-{count}", migration.anchor);
            }
            output.extend(
                lines[cursor..range.start]
                    .iter()
                    .map(|line| (*line).to_owned()),
            );
            let summary = migration.summary.replace('\n', "\n  ");
            output.push(format!("- {summary}\n"));
            output.push(format!(
                "  See the [migration instructions](/releases/{vector_version}/#{}).\n",
                migration.anchor
            ));
            if let Some(credit) = credit {
                output.push(format!("  {credit}\n"));
            }
            cursor = range.end;
            migrations.push(migration);
        }
    }
    if migrations.is_empty() {
        return Changelog {
            notes: notes.to_owned(),
            migrations,
        };
    }
    output.extend(lines[cursor..].iter().map(|line| (*line).to_owned()));
    let mut rewritten = output.join("\n");
    if notes.ends_with('\n') && !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    Changelog {
        notes: rewritten,
        migrations,
    }
}

fn migration(
    lines: &[&str],
    range: &Range<usize>,
    breaking: bool,
    anchor_prefix: Option<&str>,
) -> Option<(BreakingDetails, Option<String>)> {
    let mut body = Vec::new();
    for (offset, line) in lines[range.clone()].iter().enumerate() {
        body.push(if offset == 0 {
            line.strip_prefix("- ").unwrap_or(line)
        } else {
            line.strip_prefix("  ").unwrap_or(line)
        });
    }
    while body.last().is_some_and(|line| line.trim().is_empty()) {
        body.pop();
    }
    let credit = body.last().filter(|line| is_credit(line)).copied();
    let pr = credit.and_then(|line| {
        PULL_REQUEST
            .captures(line)
            .map(|captures| captures[1].to_owned())
    });
    if credit.is_some() {
        body.pop();
    }
    let body = body.join("\n");
    let body = body.trim_matches('\n');
    if STRUCTURED_TITLE.is_match(body.lines().next().unwrap_or("")) {
        return structured_migration(body, anchor_prefix)
            .map(|details| (details, credit.map(str::to_owned)));
    }
    if body.is_empty() || (!breaking && !has_migration_notes(body)) {
        return None;
    }
    let (summary, migration) = body.split_once("\n\n").unwrap_or((body, ""));
    let title = format!("VRL: {}", first_sentence(summary).trim_end_matches('.'));
    let anchor = pr.map_or_else(
        || crate::commands::changelog::slugify(&title),
        |pr| format!("vrl-pr-{pr}"),
    );
    Some((
        BreakingDetails {
            title,
            anchor,
            summary: summary.to_owned(),
            migration: migration_markdown(migration.trim_matches('\n')),
        },
        credit.map(str::to_owned),
    ))
}

/// Current VRL releases declare these fields explicitly; older notes use the legacy path.
fn structured_migration(body: &str, anchor_prefix: Option<&str>) -> Option<BreakingDetails> {
    let title = STRUCTURED_TITLE.captures(body.lines().next()?)?;
    let mut summary_start = None;
    for (event, range) in Parser::new(body).into_offset_iter() {
        if !matches!(event, Event::Start(Tag::Paragraph))
            || (range.start > 0 && body.as_bytes()[range.start - 1] != b'\n')
        {
            continue;
        }
        match body.get(range.clone())?.trim_end_matches('\n') {
            "**Summary**" if summary_start.is_none() => summary_start = Some(range.end),
            "**Migration**" if summary_start.is_some() => {
                let id = &title[1];
                let anchor = anchor_prefix
                    .and_then(|prefix| id.strip_prefix(prefix))
                    .unwrap_or(id);
                return Some(BreakingDetails {
                    title: format!("VRL: {}", &title[2]),
                    anchor: format!("vrl-{}", crate::commands::changelog::slugify(anchor)),
                    summary: body
                        .get(summary_start?..range.start)?
                        .trim_matches('\n')
                        .to_owned(),
                    migration: body.get(range.end..)?.trim_matches('\n').to_owned(),
                });
            }
            _ => {}
        }
    }
    None
}

fn migration_markdown(markdown: &str) -> String {
    markdown_lines(markdown)
        .map(|(line, code)| {
            if !code
                && let Some(label) = ["**", "__"].into_iter().find_map(|marker| {
                    line.strip_prefix(marker)
                        .and_then(|line| line.strip_suffix(marker))
                })
                && matches!(
                    label.trim_end_matches(':').to_ascii_lowercase().as_str(),
                    "before" | "after" | "old" | "new" | "migration" | "upgrade guide"
                )
            {
                return format!("#### {}", label.trim_end_matches(':'));
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_sentence(summary: &str) -> String {
    let summary = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    summary
        .split_once(". ")
        .map_or_else(|| summary.clone(), |(first, _)| format!("{first}."))
}

fn is_credit(line: &str) -> bool {
    let line = line.trim();
    ((line.starts_with("*Thanks to ") || line.starts_with("Thanks to "))
        && line.contains("](https://github.com/"))
        || ((line.starts_with("authors: ")
            || line.starts_with("[PR #")
            || line.starts_with("~~[PR #"))
            && PULL_REQUEST.is_match(line))
}

fn has_migration_notes(body: &str) -> bool {
    let mut before = false;
    let mut after = false;
    for (line, code) in markdown_lines(body) {
        if code || line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let label = migration_label(line);
        let matches = |name: &str| label == name || label.starts_with(&format!("{name}:"));
        if matches("migration") || matches("upgrade guide") {
            return true;
        }
        before |= matches("before") || matches("old");
        after |= matches("after") || matches("new");
    }
    before && after
}

fn migration_label(line: &str) -> String {
    line.trim()
        .trim_start_matches('#')
        .trim()
        .replace("**", "")
        .replace("__", "")
        .to_ascii_lowercase()
}

fn is_migration_heading(line: &str) -> bool {
    let label = migration_label(line);
    [
        "migration",
        "upgrade guide",
        "before",
        "after",
        "old",
        "new",
    ]
    .iter()
    .any(|name| label == *name || label.starts_with(&format!("{name}:")))
}

/// Preserve source lines while identifying code with the Markdown parser.
fn markdown_lines(markdown: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut blocks = Parser::new(markdown)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(event, Event::Start(Tag::CodeBlock(_))).then_some(range)
        })
        .peekable();
    let mut offset = 0;
    markdown.split_inclusive('\n').map(move |raw| {
        while blocks.peek().is_some_and(|range| range.end <= offset) {
            blocks.next();
        }
        let code = blocks.peek().is_some_and(|range| range.start <= offset);
        offset += raw.len();
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        (line.strip_suffix('\r').unwrap_or(line), code)
    })
}

pub(super) fn cue_block(notes: &str) -> String {
    let body = markdown_lines(notes)
        .map(|(line, code)| {
            let shift = if !code && line.starts_with('#') {
                "#"
            } else {
                ""
            };
            format!("\t\t{shift}{line}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("\tvrl_changelog: #\"\"\"\n{body}\n\t\t\"\"\"#")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_legacy_authors_credits() {
        let credit = "authors: alice bob (https://github.com/vectordotdev/vrl/pull/1557)";
        let notes = format!(
            "## [0.29.0 (2025-12-11)]\n\n### Breaking Changes & Upgrade Guide\n\n\
             - Required metadata was added.\n\n  {credit}\n"
        );
        let imported = extract(&notes, &Version::new(0, 62, 0));
        assert_eq!(imported.migrations.len(), 1);
        assert!(imported.migrations[0].migration.is_empty());
        assert_eq!(imported.migrations[0].anchor, "vrl-pr-1557");
        assert!(imported.notes.contains(credit));
    }

    #[test]
    fn imports_declared_fields_and_keeps_legacy_migrations() {
        let notes = indoc::indoc! {r#"
            ## [0.39.0 (2026-11-02)](https://github.com/vectordotdev/vrl/releases/tag/v0.39.0)
            ### Breaking Changes & Upgrade Guide
            - <a id="v0.39.0-explicit-anchor"></a> **Rename `foo`**

              **Summary**

              The full summary is preserved.
              Including its second line.

              ```text
              **Migration**
              ## [0.38.0 (example)]
              ```

              **Migration**

                  # Keep this as code
                  .foo = .bar

              *Thanks to [alice](https://github.com/alice) for contributing this change!*
            ### Fixes
            - A legacy change.

            ### Migration

            Use the replacement.

              *Thanks to [bob](https://github.com/bob) for contributing PR [#2](https://github.com/vectordotdev/vrl/pull/2)!*
            - An ordinary fix.
        "#};
        for notes in [notes.to_string(), notes.replace('\n', "\r\n")] {
            let changelog = format!("{notes}\n## [0.38.0 (2026-10-28)]\n- Earlier release.\n");
            let release = release_section(&changelog, &Version::new(0, 39, 0)).unwrap();
            let imported = extract(&release, &Version::new(0, 62, 0));
            assert_eq!(imported.migrations.len(), 2);
            let current = &imported.migrations[0];
            assert_eq!(current.title, "VRL: Rename `foo`");
            assert_eq!(current.anchor, "vrl-explicit-anchor");
            assert!(current.summary.contains("Including its second line."));
            assert!(
                current
                    .summary
                    .contains("**Migration**\n## [0.38.0 (example)]")
            );
            assert_eq!(
                current.migration,
                "    # Keep this as code\n    .foo = .bar"
            );
            assert_eq!(imported.migrations[1].anchor, "vrl-pr-2");
            assert!(
                imported.migrations[1]
                    .migration
                    .contains("Use the replacement.")
            );
            assert!(imported.notes.contains("#vrl-explicit-anchor"));
            assert!(imported.notes.contains("[alice](https://github.com/alice)"));
            assert!(imported.notes.contains("- An ordinary fix."));
            assert!(!imported.notes.contains("Earlier release."));
        }
        assert!(!crate::commands::changelog::is_valid_anchor(
            "breaking-changes"
        ));
    }
}
