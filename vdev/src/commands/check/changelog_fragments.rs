//! Best-effort validator for changelog fragments. The upgrade-guide generator stitches
//! small markdown files together into a bigger markdown file; whatever renders safely in
//! a fragment renders identically in the guide. PR review handles content quality.

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::commands::changelog::{
    is_valid_anchor, new::TODO_HANDLE, parse_breaking_sections, slugify,
};
use crate::utils::{git, paths};

const CHANGELOG_DIR: &str = "changelog.d";
const DEFAULT_MAX_FRAGMENTS: usize = 1000;

/// Allowed changelog fragment types.
///
/// NOTE: keep this list in sync with `vdev/src/commands/release/generate_cue.rs`
/// and `changelog.d/README.md`.
const FRAGMENT_TYPES: &[&str] = &["breaking", "security", "feature", "enhancement", "fix"];

/// Validate changelog fragments added on this branch/PR.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Merge base to diff against.
    #[arg(long, default_value = "origin/master")]
    merge_base: String,

    /// Maximum number of fragments accepted in a single PR.
    #[arg(long, default_value_t = DEFAULT_MAX_FRAGMENTS)]
    max_fragments: usize,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = paths::find_repo_root()?;
        let changelog_dir = repo_root.join(CHANGELOG_DIR);
        if !changelog_dir.is_dir() {
            bail!(
                "No {CHANGELOG_DIR}/ directory at {}. Run this from the Vector repo root.",
                repo_root.display()
            );
        }

        // The gate "did the PR add a new fragment?" uses A (added) only — modifying an
        // existing on-master fragment doesn't count toward the required changelog entry.
        // Schema validation, on the other hand, must see M (modified) fragments too.
        let is_real_fragment =
            |p: &PathBuf| p.file_name().and_then(|s| s.to_str()) != Some("README.md");
        let added_real: Vec<PathBuf> = diff_fragments(&self.merge_base, "A")?
            .into_iter()
            .filter(is_real_fragment)
            .collect();
        if added_real.is_empty() {
            bail!(
                "No changelog fragments detected. \
                 If no changes necessitate user-facing explanations, add the 'no-changelog' label. \
                 Otherwise, add fragments to {CHANGELOG_DIR}/ (see {CHANGELOG_DIR}/README.md)."
            );
        }
        if added_real.len() > self.max_fragments {
            bail!(
                "Too many changelog fragments ({} > {}).",
                added_real.len(),
                self.max_fragments
            );
        }

        // Every touched fragment (added or modified) must pass the schema check.
        let modified: Vec<PathBuf> = diff_fragments(&self.merge_base, "M")?
            .into_iter()
            .filter(is_real_fragment)
            .collect();
        let expected_parent = std::path::Path::new(CHANGELOG_DIR);
        for path in added_real.iter().chain(modified.iter()) {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                bail!("Unexpected fragment path: {}", path.display());
            };
            if path.parent() != Some(expected_parent) {
                bail!(
                    "invalid fragment path '{}': fragments must live directly under {CHANGELOG_DIR}/, not in a subdirectory.",
                    path.display()
                );
            }
            info!("Validating '{name}'");
            let fragment_type = validate_filename(name)?;
            validate_contents(&repo_root.join(path), name, fragment_type)?;
        }

        // Cross-fragment check: derived-or-explicit anchors across every breaking fragment
        // currently in `changelog.d/` must be unique and non-empty. Catches conflicts at CI
        // time rather than at release time (when a partial CUE may already exist).
        validate_breaking_anchor_set(&changelog_dir)?;

        info!("changelog additions are valid.");
        Ok(())
    }
}

/// Load every `*.breaking.md` in `changelog.d/`, compute its anchor (explicit override or
/// slugified title), and verify the resulting set is non-empty, well-formed, and unique.
fn validate_breaking_anchor_set(changelog_dir: &std::path::Path) -> Result<()> {
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for entry in std::fs::read_dir(changelog_dir)? {
        let path = entry?.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) if n.ends_with(".breaking.md") => n.to_string(),
            _ => continue,
        };
        let content = std::fs::read_to_string(&path)?;
        let body_before_authors = content
            .rsplit_once("\nauthors: ")
            .map_or(content.as_str(), |(before, _)| before);
        let sections = parse_breaking_sections(body_before_authors)
            .map_err(|e| anyhow::anyhow!("invalid breaking fragment '{name}': {e}"))?;
        let anchor = sections.anchor.unwrap_or_else(|| slugify(&sections.title));
        if !is_valid_anchor(&anchor) {
            bail!(
                "invalid breaking fragment '{name}': derived anchor '{anchor}' is not a valid kebab-case slug (add an explicit `{{#some-slug}}` after the title)."
            );
        }
        if let Some(other) = seen.insert(anchor.clone(), name.clone()) {
            bail!(
                "duplicate upgrade-guide anchor '#{anchor}' shared by breaking fragments '{other}' and '{name}'. Override one with an explicit `{{#unique-slug}}`."
            );
        }
    }
    Ok(())
}

/// `git diff --name-only --diff-filter=<filter> --merge-base <merge_base> changelog.d`
///
/// `filter` is a `git diff` `--diff-filter` value: `A` for added-only, `M` for
/// modified-only, `AM` for both, etc.
fn diff_fragments(merge_base: &str, filter: &str) -> Result<Vec<PathBuf>> {
    let filter_arg = format!("--diff-filter={filter}");
    let out = git::run_and_check_output(&[
        "diff",
        "--name-only",
        &filter_arg,
        "--merge-base",
        merge_base,
        CHANGELOG_DIR,
    ])?;
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(PathBuf::from)
        .collect())
}

fn validate_filename(filename: &str) -> Result<&'static str> {
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() != 3 {
        bail!(
            "invalid fragment filename '{filename}': expected '<unique_name>.<fragment_type>.md'"
        );
    }
    let fragment_type = parts[1];
    let Some(known) = FRAGMENT_TYPES.iter().find(|t| **t == fragment_type) else {
        bail!(
            "invalid fragment filename '{filename}': fragment type must be one of ({}).",
            FRAGMENT_TYPES.join("|")
        );
    };
    if parts[2] != "md" {
        bail!("invalid fragment filename '{filename}': extension must be markdown (.md).");
    }
    Ok(*known)
}

fn validate_contents(path: &std::path::Path, filename: &str, fragment_type: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    // Match generate_cue.rs, which reads `lines().last()` verbatim: the authors
    // line must be the last line, no trailing blank lines allowed.
    let last_line = content.lines().last().unwrap_or("");

    let Some(names) = last_line.strip_prefix("authors: ") else {
        bail!(
            "invalid fragment contents for '{filename}': last line must be 'authors: <name> [<name> ...]' (no trailing blank lines)."
        );
    };
    let names = names.trim();
    if names.is_empty() {
        bail!("invalid fragment contents for '{filename}': authors line has no names.");
    }
    if names.contains('@') {
        bail!(
            "invalid fragment contents for '{filename}': author names should not be prefixed with '@'."
        );
    }
    if names.contains(',') {
        bail!(
            "invalid fragment contents for '{filename}': authors should be space delimited, not comma delimited."
        );
    }
    if names.split_whitespace().any(|n| n == TODO_HANDLE) {
        bail!(
            "invalid fragment contents for '{filename}': the scaffolder placeholder '{TODO_HANDLE}' must be replaced with a real GitHub handle."
        );
    }

    // Reject any scaffolded body placeholder that the author forgot to fill in.
    // All templates start their placeholder lines with a literal `TODO ` at column 0,
    // which is rare in real fragment text.
    if let Some(bad) = content.lines().find(|l| l.starts_with("TODO ")) {
        bail!(
            "invalid fragment contents for '{filename}': scaffolder placeholder line still present ({bad:?}). Replace it with real content."
        );
    }

    if fragment_type == "breaking" {
        validate_breaking_fragment(&content, filename)?;
    }

    Ok(())
}

/// Schema for `*.breaking.md`:
///
/// ```text
/// # <Title> [{#optional-anchor}]
///
/// ## Summary
///
/// <markdown — one paragraph, lands in the release changelog list>
///
/// ## Migration
///
/// <markdown — "Action needed" body for the upgrade guide, or "N/A">
///
/// authors: <name> [<name> ...]
/// ```
fn validate_breaking_fragment(content: &str, filename: &str) -> Result<()> {
    // Strip the trailing `authors:` line before parsing — it's not part of the markdown
    // structure and would otherwise land inside the Migration section.
    let body_before_authors = content
        .rsplit_once("\nauthors: ")
        .map_or(content, |(before, _)| before);

    let sections = parse_breaking_sections(body_before_authors)
        .map_err(|e| anyhow::anyhow!("invalid breaking fragment '{filename}': {e}"))?;

    if sections.title.is_empty() {
        bail!("invalid breaking fragment '{filename}': title must not be empty.");
    }
    if sections.title.contains("TODO") {
        bail!(
            "invalid breaking fragment '{filename}': title still contains the scaffolder placeholder 'TODO'."
        );
    }
    if let Some(anchor) = sections.anchor.as_deref()
        && !is_valid_anchor(anchor)
    {
        bail!(
            "invalid breaking fragment '{filename}': anchor must be a lowercase kebab-case slug (a-z, 0-9, hyphens); got '{anchor}'."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::{formatdoc, indoc};

    /// Build a full breaking-fragment file body from the given title line and section content.
    fn wrap(title_line: &str, summary: &str, migration: &str) -> String {
        formatdoc! {"
            {title_line}

            ## Summary

            {summary}

            ## Migration

            {migration}

            authors: pront
        "}
    }

    #[test]
    fn valid_breaking_fragment() {
        let raw = wrap(
            "# Env var interpolation disabled {#env-var}",
            "Off by default now.",
            "Pass the flag.",
        );
        validate_breaking_fragment(&raw, "x.breaking.md").unwrap();
    }

    #[test]
    fn valid_without_anchor() {
        let raw = wrap("# A change", "Change happened.", "N/A");
        validate_breaking_fragment(&raw, "x.breaking.md").unwrap();
    }

    #[test]
    fn missing_title() {
        let raw = indoc! {"
            not an H1 line

            ## Summary

            x

            ## Migration

            N/A

            authors: pront
        "};
        let err = validate_breaking_fragment(raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("H1 title"), "{err}");
    }

    #[test]
    fn empty_title() {
        let raw = wrap("# ", "x", "N/A");
        let err = validate_breaking_fragment(&raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("title must not be empty"), "{err}");
    }

    #[test]
    fn missing_summary_header() {
        let raw = indoc! {"
            # x

            ## Migration

            N/A

            authors: pront
        "};
        let err = validate_breaking_fragment(raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("`## Summary`"), "{err}");
    }

    #[test]
    fn missing_migration_header() {
        let raw = indoc! {"
            # x

            ## Summary

            y

            authors: pront
        "};
        let err = validate_breaking_fragment(raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("`## Migration`"), "{err}");
    }

    #[test]
    fn empty_summary_section() {
        let raw = wrap("# x", "", "N/A");
        let err = validate_breaking_fragment(&raw, "x.breaking.md").unwrap_err();
        assert!(
            err.to_string().contains("Summary` must not be empty"),
            "{err}"
        );
    }

    #[test]
    fn empty_migration_section_is_allowed() {
        // Fragments may leave `## Migration` empty (nothing for the user to do).
        let raw = wrap("# x", "y", "");
        validate_breaking_fragment(&raw, "x.breaking.md").unwrap();
    }

    #[test]
    fn wrong_section_order() {
        let raw = indoc! {"
            # x

            ## Migration

            do this

            ## Summary

            hi

            authors: pront
        "};
        let err = validate_breaking_fragment(raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("must come before"), "{err}");
    }

    #[test]
    fn bad_anchor() {
        let raw = wrap("# x {#Not Valid!}", "y", "N/A");
        let err = validate_breaking_fragment(&raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("kebab-case slug"), "{err}");
    }

    #[test]
    fn todo_title_rejected() {
        let raw = wrap("# TODO one-line title", "y", "N/A");
        let err = validate_breaking_fragment(&raw, "x.breaking.md").unwrap_err();
        assert!(err.to_string().contains("placeholder 'TODO'"), "{err}");
    }
}
