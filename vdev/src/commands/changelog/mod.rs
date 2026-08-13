pub(crate) mod new;

use anyhow::{Result, bail};

crate::cli_subcommands! {
    "Scaffold and inspect changelog fragments..."
    new,
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

/// Anchors that would collide with fixed headings emitted by the upgrade-guide renderer.
/// Fragments may not choose these as their `{#anchor}` override, or produce them via slugify.
pub(crate) const RESERVED_ANCHORS: &[&str] = &["vector-breaking-changes", "vector-upgrade-guide"];

/// Slug rule shared by the CI checker and the release generator: lowercase ASCII alnum
/// with `-` separators, non-empty, no leading/trailing hyphen, not a reserved name.
pub(crate) fn is_valid_anchor(anchor: &str) -> bool {
    !anchor.is_empty()
        && !RESERVED_ANCHORS.contains(&anchor)
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
        let s = parse_breaking_sections(body).unwrap();
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

    #[test]
    fn parses_minimal_fragment_without_anchor() {
        let body = indoc! {"
            # A change

            ## Summary

            Summary.

            ## Migration
        "};
        let s = parse_breaking_sections(body).unwrap();
        assert_eq!(s.title, "A change");
        assert_eq!(s.anchor, None);
        assert_eq!(s.summary, "Summary.");
        assert_eq!(s.migration, "");
    }

    #[test]
    fn missing_title_errors() {
        let body = "## Summary\n\nx\n\n## Migration\n\ny\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(err.to_string().contains("H1 title"), "{err}");
    }

    #[test]
    fn empty_title_errors() {
        let body = "# \n\n## Summary\n\nx\n\n## Migration\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(err.to_string().contains("must not be empty"), "{err}");
    }

    #[test]
    fn missing_summary_errors() {
        let body = "# x\n\n## Migration\n\ny\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(err.to_string().contains("`## Summary`"), "{err}");
    }

    #[test]
    fn missing_migration_errors() {
        let body = "# x\n\n## Summary\n\ny\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(err.to_string().contains("`## Migration`"), "{err}");
    }

    #[test]
    fn empty_summary_errors() {
        let body = "# x\n\n## Summary\n\n## Migration\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(
            err.to_string().contains("Summary` must not be empty"),
            "{err}"
        );
    }

    #[test]
    fn empty_migration_is_allowed() {
        let body = "# x\n\n## Summary\n\nSummary.\n\n## Migration\n";
        let s = parse_breaking_sections(body).unwrap();
        assert_eq!(s.migration, "");
    }

    #[test]
    fn migration_before_summary_errors() {
        let body = "# x\n\n## Migration\n\ny\n\n## Summary\n\nz\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(err.to_string().contains("must come before"), "{err}");
    }

    #[test]
    fn crlf_fragment_parses() {
        let body = "# x\r\n\r\n## Summary\r\n\r\ny\r\n\r\n## Migration\r\n\r\nz\r\n";
        let s = parse_breaking_sections(body).unwrap();
        assert_eq!(s.title, "x");
        assert_eq!(s.summary, "y");
        assert_eq!(s.migration, "z");
    }

    #[test]
    fn duplicate_section_marker_errors() {
        let body = "# x\n\n## Summary\n\ny\n\n## Summary\n\nz\n\n## Migration\n";
        let err = parse_breaking_sections(body).unwrap_err();
        assert!(err.to_string().contains("exactly one"), "{err}");
    }

    #[test]
    fn split_title_and_anchor_extracts_anchor() {
        let (t, a) = split_title_and_anchor("Some Title {#my-anchor}");
        assert_eq!(t, "Some Title");
        assert_eq!(a, Some("my-anchor"));
    }

    #[test]
    fn split_title_and_anchor_without_anchor() {
        let (t, a) = split_title_and_anchor("Some Title");
        assert_eq!(t, "Some Title");
        assert_eq!(a, None);
    }
}
