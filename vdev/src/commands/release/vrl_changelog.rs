//! Import VRL release notes and extract their migration instructions before publishing.

use std::{collections::HashMap, ops::Range, process::Command, sync::LazyLock};

use anyhow::{Context, Result, bail};
use regex::Regex;
use semver::Version;

use super::generate_cue::BreakingDetails;

static PULL_REQUEST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https://github\.com/vectordotdev/vrl/pull/([0-9]+)")
        .expect("valid VRL pull-request regex")
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
    let mut fence = None;
    for line in changelog.lines() {
        if !in_fence(line, &mut fence) && line.starts_with("## ") {
            if !section.is_empty() {
                break;
            }
            if !line.starts_with(&prefix) {
                continue;
            }
        }
        if !section.is_empty() || line.starts_with(&prefix) {
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
    let mut fence = None;
    for (index, line) in lines.iter().enumerate() {
        if in_fence(line, &mut fence) {
            continue;
        }
        if line.starts_with("- ") || line.starts_with("## ") || line.starts_with("### ") {
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
        if let Some((mut migration, credit)) = migration(&lines, &range, breaking) {
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
            let summary = migration
                .summary
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
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
    let body = body.trim();
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
            migration: migration_markdown(migration.trim()),
        },
        credit.map(str::to_owned),
    ))
}

fn migration_markdown(markdown: &str) -> String {
    let mut fence = None;
    markdown
        .lines()
        .map(|line| {
            if !in_fence(line, &mut fence)
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
    (line.starts_with("*Thanks to ")
        || line.starts_with("Thanks to ")
        || line.starts_with("[PR #")
        || line.starts_with("~~[PR #"))
        && PULL_REQUEST.is_match(line)
}

fn has_migration_notes(body: &str) -> bool {
    let mut before = false;
    let mut after = false;
    let mut fence = None;
    for line in body.lines() {
        if in_fence(line, &mut fence) || line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let label = line.trim().trim_start_matches('#').trim();
        let label = label
            .replace("**", "")
            .replace("__", "")
            .to_ascii_lowercase();
        let matches = |name: &str| label == name || label.starts_with(&format!("{name}:"));
        if matches("migration") || matches("upgrade guide") {
            return true;
        }
        before |= matches("before") || matches("old");
        after |= matches("after") || matches("new");
    }
    before && after
}

/// True for every line inside a fence, including its opening and closing markers.
/// Shared by section/entry detection and migration-label detection.
fn in_fence(line: &str, fence: &mut Option<(u8, usize)>) -> bool {
    let line = line.trim_start();
    let marker = line
        .as_bytes()
        .first()
        .copied()
        .filter(|byte| matches!(byte, b'`' | b'~'));
    if let Some(marker) = marker {
        let length = line.bytes().take_while(|byte| *byte == marker).count();
        if length >= 3 {
            match *fence {
                Some((open_marker, open_length))
                    if marker == open_marker
                        && length >= open_length
                        && line
                            .get(length..)
                            .is_some_and(|tail| tail.trim().is_empty()) =>
                {
                    *fence = None;
                }
                None => *fence = Some((marker, length)),
                _ => {}
            }
            return true;
        }
    }
    fence.is_some()
}

pub(super) fn cue_block(notes: &str) -> String {
    let mut fence = None;
    let body = notes
        .lines()
        .map(|line| {
            let code = in_fence(line, &mut fence);
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
    use indoc::indoc;

    const VRL_038: &str = include_str!("fixtures/vrl-0.38.0.txt");
    const VRL_036: &str = include_str!("fixtures/vrl-0.36.0.txt");

    #[test]
    fn released_fix_migration_is_extracted_with_all_examples_and_credit_preserved() {
        let result = extract(VRL_038, &Version::new(0, 61, 0));
        assert_eq!(result.migrations.len(), 1);
        let migration = &result.migrations[0];
        assert_eq!(migration.anchor, "vrl-pr-2001");
        assert!(migration.summary.contains("correctly marked as fallible"));
        assert!(
            migration
                .migration
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("no longer compile until the error is handled")
        );
        assert_eq!(migration.migration.matches("```vrl").count(), 3);
        assert!(migration.migration.contains("#### Before"));
        assert!(migration.migration.contains("#### After"));
        assert!(!result.notes.contains("```vrl"));
        assert!(result.notes.contains(concat!(
            r#"`to_unix_timestamp` with `unit: "nanoseconds"` is now correctly marked as fallible by the compiler. "#,
            "The conversion can fail at runtime when the timestamp is out of range for a ",
            "64-bit nanosecond value, but the compiler previously rejected error handling for this ",
            "call as unnecessary."
        )));
        assert!(!result.notes.contains("Programs that call"));
        assert!(result.notes.contains("/releases/0.61.0/#vrl-pr-2001"));
        assert_eq!(result.notes.matches("*Thanks to ").count(), 6);
        assert!(result.notes.contains("### New Features"));
        assert!(result.notes.contains("### Fixes"));
    }

    #[test]
    fn released_breaking_section_is_extracted_without_changing_ordinary_entries() {
        let result = extract(VRL_036, &Version::new(0, 59, 0));
        assert_eq!(result.migrations.len(), 1);
        assert_eq!(result.migrations[0].anchor, "vrl-pr-1861");
        assert!(
            result.migrations[0]
                .migration
                .contains("Passing the wrong element type")
        );
        assert!(result.notes.contains("/releases/0.59.0/#vrl-pr-1861"));
        let ordinary = VRL_036.split("### New Features").nth(1).unwrap();
        assert!(result.notes.ends_with(ordinary));
    }

    #[test]
    fn fences_and_nested_lists_do_not_split_entries_or_invent_migration_labels() {
        let notes = indoc! {r"
            ## [1.0.0 (2026-10-30)](https://example.com)
            ### Fixes
            - Keep this example in the changelog.

              ````markdown
              ### Breaking Changes
              - A fake entry.
              **Before**
              ```
              **After**
              ````

              *Thanks to [author](https://github.com/author) for contributing PR [#1](https://github.com/vectordotdev/vrl/pull/1)!*
            ### Breaking Changes & Upgrade Guide
            - Remove an obsolete option.

              Set the replacement:
              - Keep this nested instruction.
                - And its nested detail.

              ~~~vrl
              # Keep comments as written.
              - a_code_example
              ~~~

              [PR #2](https://github.com/vectordotdev/vrl/pull/2) by [@author](https://github.com/author)
        "};
        let result = extract(notes, &Version::new(1, 0, 0));
        assert_eq!(result.migrations.len(), 1);
        assert_eq!(result.migrations[0].anchor, "vrl-pr-2");
        assert!(
            result.migrations[0]
                .migration
                .contains("- Keep this nested instruction.\n  - And its nested detail.")
        );
        assert!(result.notes.contains("- A fake entry."));
        assert!(result.notes.contains("[PR #2]"));
        assert!(cue_block(&result.notes).contains("\t\t  ### Breaking Changes"));
    }

    #[test]
    fn unchanged_notes_and_explicit_migration_headings() {
        let ordinary = "## [1.0.0 (2026-10-30)](https://example.com)\n### Fixes\n- Fix a panic.\n";
        assert_eq!(extract(ordinary, &Version::new(1, 0, 0)).notes, ordinary);
        let migration = format!("{ordinary}\n  #### Migration\n\n  Enable the new option.\n");
        let result = extract(&migration, &Version::new(1, 0, 0));
        assert_eq!(result.migrations.len(), 1);
        assert!(
            result.migrations[0]
                .migration
                .contains("Enable the new option.")
        );
    }

    #[test]
    fn fetch_selects_the_requested_release_instead_of_the_first_heading() {
        let notes = format!("## Unreleased\n\n### Fixes\n- Future fix.\n\n{VRL_038}\n{VRL_036}");
        let result = release_section(&notes, &Version::new(0, 38, 0)).unwrap();
        assert!(result.starts_with("## [0.38.0 "));
        assert!(!result.contains("Future fix"));
        assert!(!result.contains("## [0.36.0 "));
        assert!(release_section(&notes, &Version::new(9, 0, 0)).is_err());
    }

    #[test]
    fn multiple_migrations_from_one_pr_have_distinct_stable_links() {
        let entry = |summary: &str| {
            format!(
                "- {summary}\n\n  Replace the option.\n\n  *Thanks to [author](https://github.com/author) for contributing PR [#42](https://github.com/vectordotdev/vrl/pull/42)!*\n"
            )
        };
        let notes = format!(
            "## [1.0.0 (2026-10-30)](https://example.com)\n### Breaking Changes\n{}\n{}",
            entry("First removal."),
            entry("Second removal.")
        );
        let result = extract(&notes, &Version::new(1, 0, 0));
        assert_eq!(result.migrations.len(), 2);
        assert_eq!(result.migrations[0].anchor, "vrl-pr-42");
        assert_eq!(result.migrations[1].anchor, "vrl-pr-42-2");
        assert!(result.notes.contains("#vrl-pr-42)"));
        assert!(result.notes.contains("#vrl-pr-42-2)"));
    }

    #[test]
    fn migration_labels_become_headings_without_rewriting_fenced_code() {
        assert_eq!(
            migration_markdown("__before__\n\n**Migration**"),
            "#### before\n\n#### Migration"
        );
        assert!(!has_migration_notes(
            "Example:\n\n    **Before**\n    **After**"
        ));
        let markdown = "**Before**\n\n~~~markdown\n**After**\n~~~\n\n**After:**";
        assert_eq!(
            migration_markdown(markdown),
            "#### Before\n\n~~~markdown\n**After**\n~~~\n\n#### After"
        );
    }

    #[test]
    fn cue_heading_shift_preserves_code_indentation_and_comment_lines() {
        let notes =
            "## VRL\n### Fixes\n- Example\n\n  ```vrl\n  # comment\n    nested = true\n  ```";
        let block = cue_block(notes);
        assert!(block.contains("\t\t### VRL\n\t\t#### Fixes"));
        assert!(block.contains("\t\t  # comment\n\t\t    nested = true"));
    }
}
