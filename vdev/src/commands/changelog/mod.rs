pub(crate) mod new;
pub(crate) mod types;

use anyhow::{Result, bail};

/// A valid changelog fragment type and its user-facing description.
pub struct FragmentType {
    pub name: &'static str,
    pub description: &'static str,
    /// The type emitted for this fragment in the generated release CUE file.
    pub cue_type: &'static str,
    /// Whether fragments of this type use the structured breaking-fragment layout
    /// (`## Summary` / `## Migration`).
    pub breaking: bool,
}

/// Valid changelog fragment types and their descriptions. Single source of truth shared by
/// the scaffolder (`vdev changelog new`), the CI checker (`vdev check changelog-fragments`),
/// the release CUE generator, and `vdev changelog types`.
pub const FRAGMENT_TYPES: &[FragmentType] = &[
    FragmentType {
        name: "breaking",
        description: "A change that is incompatible with prior versions and requires users to make adjustments. If a change is also a fix or feature, breaking takes precedence.",
        cue_type: "chore",
        breaking: true,
    },
    FragmentType {
        name: "security",
        description: "A change that has security implications.",
        cue_type: "security",
        breaking: false,
    },
    FragmentType {
        name: "feature",
        description: "A change that introduces a new feature.",
        cue_type: "feat",
        breaking: false,
    },
    FragmentType {
        name: "enhancement",
        description: "A change that enhances existing functionality in a user perceivable way.",
        cue_type: "enhancement",
        breaking: false,
    },
    FragmentType {
        name: "fix",
        description: "A change that fixes a bug.",
        cue_type: "fix",
        breaking: false,
    },
];

crate::cli_subcommands! {
    "Scaffold and inspect changelog fragments..."
    new,
    types,
}

/// Structured view of a `*.breaking.md` fragment. Shared by the checker and the release
/// CUE generator so both agree on what counts as title / Summary / Migration.
#[derive(Debug)]
pub(crate) struct BreakingSections {
    pub title: String,
    pub anchor: Option<String>,
    pub summary: String,
    pub migration: String,
}

const SUMMARY_MARKER: &str = "\n## Summary\n";
const MIGRATION_MARKER: &str = "\n## Migration\n";

/// Fixed release-page headings and structural IDs that migration anchors must not shadow.
/// Fragments may not choose these as their `{#anchor}` override, or produce them via slugify.
pub(crate) const RESERVED_ANCHORS: &[&str] = &[
    "breaking-changes",
    "bug-fixes",
    "changelog",
    "chore",
    "deprecation-announcements",
    "deprecations",
    "enhancements",
    "footer-heading",
    "known-issues",
    "new-features",
    "page-content",
    "security-changes",
    "toc",
    "upgrade-guide",
    "vector-breaking-changes",
    "vector-changelog",
    "vector-upgrade-guide",
    "vrl-breaking-changes",
    "vrl-breaking-changes--upgrade-guide",
    "vrl-changelog",
    "vrl-enhancements",
    "vrl-fixes",
    "vrl-new-features",
    "vrl-security",
    "whats-next",
];

/// Slug rule shared by the CI checker and the release generator: lowercase ASCII alnum
/// with `-` separators, non-empty, no leading/trailing hyphen, not a reserved name.
/// The renderer owns `upgrade-guide-*` for migration subheadings.
pub(crate) fn is_valid_anchor(anchor: &str) -> bool {
    !anchor.is_empty()
        && !RESERVED_ANCHORS.contains(&anchor)
        && !anchor.starts_with("upgrade-guide-")
        && !anchor.starts_with('-')
        && !anchor.ends_with('-')
        && anchor
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Kebab-case slugify — the derivation used when a breaking fragment omits `{#anchor}`.
pub(crate) fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_dash = true; // suppress leading dash
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Parse a breaking-fragment body (H1 title + `## Summary` + `## Migration`) via plain
/// string splits. Only the H1 title, the `## Summary` header, and the `## Migration` header
/// are mandatory; content inside each section is returned verbatim (any sub-headings the
/// author wrote — `### Old`, `### New`, whatever — pass through untouched).
pub(crate) fn parse_breaking_sections(body: &str) -> Result<BreakingSections> {
    // Normalize CRLF → LF so Windows checkouts parse identically. Also pad the input so
    // section markers like `\n## Migration\n` still match when the last section is empty
    // and the file ends right after the header line.
    let mut normalized = body.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    let body: &str = &normalized;

    // 1) File must begin with an H1 title on the first line — no leading blank lines.
    let title_body = body
        .strip_prefix("# ")
        .ok_or_else(|| anyhow::anyhow!("first line must be an H1 title (`# ...`)"))?;
    let (title_line, after_title) = title_body.split_once('\n').unwrap_or((title_body, ""));
    let (title_text, anchor) = split_title_and_anchor(title_line);
    let title = title_text.to_string();
    if title.is_empty() {
        bail!("H1 title must not be empty");
    }

    // 2) Exactly one `## Summary` and one `## Migration`, in that order.
    if after_title.matches(SUMMARY_MARKER).count() != 1 {
        bail!("exactly one `## Summary` section is required");
    }
    if after_title.matches(MIGRATION_MARKER).count() != 1 {
        bail!("exactly one `## Migration` section is required");
    }
    let s_pos = after_title.find(SUMMARY_MARKER).unwrap();
    let m_pos = after_title.find(MIGRATION_MARKER).unwrap();
    if s_pos >= m_pos {
        bail!("`## Summary` must come before `## Migration`");
    }

    // Reject any prose between the H1 title and `## Summary`. Someone hand-migrating an
    // old free-form breaking fragment could carry over the previous body here and it
    // would silently disappear from both the release CUE and the upgrade guide.
    // Everything before Summary must be whitespace (`\n## Summary\n` starts with the
    // leading newline, so `s_pos` includes it).
    if let Some(prefix) = after_title.get(..s_pos)
        && !prefix.trim().is_empty()
    {
        bail!(
            "content between the title and `## Summary` is not allowed — move it into `## Summary` or `## Migration`."
        );
    }

    let summary = after_title
        .get(s_pos + SUMMARY_MARKER.len()..m_pos)
        .unwrap_or("")
        .trim()
        .to_string();
    let migration = after_title
        .get(m_pos + MIGRATION_MARKER.len()..)
        .unwrap_or("")
        .trim()
        .to_string();

    if summary.is_empty() {
        bail!("`## Summary` must not be empty");
    }

    Ok(BreakingSections {
        title,
        anchor: anchor.map(str::to_string),
        summary,
        migration,
    })
}

/// Split `"Title Text {#anchor}"` into `("Title Text", Some("anchor"))`.
/// The `{#anchor}` suffix is a `Hugo`-style attribute list; `CommonMark` treats it as
/// plain text so we peel it off here.
pub(crate) fn split_title_and_anchor(line: &str) -> (&str, Option<&str>) {
    let trimmed = line.trim_end();
    if let Some(prefix) = trimmed.strip_suffix('}')
        && let Some(open) = prefix.rfind("{#")
    {
        let title = prefix.get(..open).unwrap_or("").trim();
        let anchor = prefix.get(open + 2..).unwrap_or("");
        return (title, Some(anchor));
    }
    (trimmed, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parses_full_breaking_fragment() {
        let body = indoc! {"
            # Env var interpolation off {#env-var}

            ## Summary

            Off by default now.

            ## Migration

            Pass the flag.

            ### Old

            ```bash
            vector --config vector.yaml
            ```

            ### New

            ```bash
            vector --config vector.yaml --dangerously-allow-env-var-interpolation
            ```
        "};
        for body in [body.to_owned(), body.replace('\n', "\r\n")] {
            let s = parse_breaking_sections(&body).unwrap();
            assert_eq!(s.title, "Env var interpolation off");
            assert_eq!(s.anchor.as_deref(), Some("env-var"));
            assert_eq!(s.summary, "Off by default now.");
            assert!(s.migration.contains("### Old"));
            assert!(s.migration.contains("### New"));
            assert!(
                s.migration
                    .contains("--dangerously-allow-env-var-interpolation")
            );
        }
    }

    #[test]
    fn parses_minimal_fragment_without_anchor() {
        let body = indoc! {"
            # A change

            ## Summary

            Summary.

            ## Migration
        "};
        for body in [
            body.to_owned(),
            body.trim_end().to_owned(),
            body.replace('\n', "\r\n"),
        ] {
            let sections = parse_breaking_sections(&body).unwrap();
            assert_eq!(sections.title, "A change");
            assert_eq!(sections.anchor, None);
            assert_eq!(sections.summary, "Summary.");
            assert_eq!(sections.migration, "");
        }
    }

    #[test]
    fn rejects_invalid_breaking_sections() {
        for (body, expected) in [
            ("## Summary\n\nx\n\n## Migration\n\ny\n", "H1 title"),
            (
                "# \n\n## Summary\n\nx\n\n## Migration\n",
                "must not be empty",
            ),
            ("# x\n\n## Migration\n\ny\n", "`## Summary`"),
            ("# x\n\n## Summary\n\ny\n", "`## Migration`"),
            (
                "# x\n\n## Summary\n\n## Migration\n",
                "Summary` must not be empty",
            ),
            (
                "# x\n\n## Migration\n\ny\n\n## Summary\n\nz\n",
                "must come before",
            ),
            (
                "# x\n\n## Summary\n\ny\n\n## Summary\n\nz\n\n## Migration\n",
                "exactly one",
            ),
        ] {
            let error = parse_breaking_sections(body).unwrap_err();
            assert!(error.to_string().contains(expected), "{body:?}: {error}");
        }
    }

    #[test]
    fn derives_anchor_slugs() {
        for (title, expected) in [
            ("A Big Change!", "a-big-change"),
            ("  --Foo/Bar--  ", "foo-bar"),
            ("already-good", "already-good"),
            ("Numbers 123 OK", "numbers-123-ok"),
        ] {
            assert_eq!(slugify(title), expected, "{title}");
        }
    }
}
