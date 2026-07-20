use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use regex::Regex;
use semver::Version;
use serde_json::json;

use crate::utils::{git, paths};

const RELEASES_DIR: &str = "website/cue/reference/releases";
const CHANGELOG_DIR: &str = "changelog.d";
const HIGHLIGHTS_DIR: &str = "website/content/en/highlights";

/// Allowed conventional-commit types.
const ALLOWED_TYPES: &[&str] = &[
    "chore",
    "docs",
    "feat",
    "fix",
    "enhancement",
    "perf",
    "revert",
    "security",
];

/// Generate the release CUE file (and, if there are breaking fragments, the upgrade guide)
/// for the given version. Handy for testing the changelog pipeline without running the
/// full `release prepare` flow.
///
/// This subcommand is generation-only: it never mutates `changelog.d/`. `release prepare`
/// invokes `retire_changelog_fragments` as a separate follow-up step.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// The version being released (e.g. `0.58.0`).
    #[arg(long)]
    version: Version,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        run(&self.version)?;
        Ok(())
    }
}

/// Generate the release CUE file for the given new version. Returns the path that was written.
///
/// Pure generation: does not touch `changelog.d/`. Callers that want the fragments retired
/// after a successful release run should call [`retire_all_fragments`] afterward.
pub(super) fn run(new_version: &Version) -> Result<PathBuf> {
    let repo_root = paths::find_repo_root()?;
    env::set_current_dir(&repo_root)?;

    info!("Creating release meta file...");

    let last_version = git::latest_release_version()?;
    let commits = fetch_commits_since(&last_version)?;

    validate_single_bump(&last_version, new_version)?;
    let new_version = new_version.clone();

    // Capture today's date ONCE so the highlights filename and its frontmatter can never
    // disagree if the run crosses UTC midnight.
    let today = Utc::now().format("%Y-%m-%d").to_string();

    let cue_path = repo_root
        .join(RELEASES_DIR)
        .join(format!("{new_version}.cue"));
    let highlights_path = repo_root
        .join(HIGHLIGHTS_DIR)
        .join(upgrade_guide_filename(&today, &new_version));

    if cue_path.exists() {
        bail!(
            "{} already exists. Delete it (or move it aside) and re-run.",
            cue_path.display()
        );
    }
    // Note: the highlights collision is checked later, only if we're actually going to
    // write it — so a manually-authored upgrade guide doesn't block a release with no
    // breaking fragments.

    // Drop any commits that have already been recorded in a previous
    // release CUE file. `--cherry-pick --right-only` only catches
    // patch-id-equivalent commits, so non-identical backports of the same
    // change (different SHA, same PR number) can otherwise re-appear in the
    // next release CUE.
    let already_released = collect_released_identifiers(&repo_root.join(RELEASES_DIR))?;
    let commits: Vec<Commit> = commits
        .into_iter()
        .filter(|c| {
            !already_released.shas.contains(&c.sha)
                && c.pr_number
                    .is_none_or(|pr| !already_released.pr_numbers.contains(&pr))
        })
        .collect();

    if commits.is_empty() {
        bail!("No commits found since v{last_version}; nothing to release.");
    }

    for c in &commits {
        c.validate()?;
    }

    let changelog_dir = repo_root.join(CHANGELOG_DIR);
    let changelog_entries = read_changelog_fragments(&changelog_dir)?;

    // Validate + render everything IN MEMORY before touching disk, so a validation
    // failure doesn't leave a partial CUE file behind (which would then trip the
    // "file already exists" guard on the next run).
    let cue_text = render_release_cue(&new_version, &changelog_entries, &commits);
    let breaking: Vec<&BreakingDetails> = changelog_entries
        .iter()
        .filter_map(|e| e.breaking_details.as_ref())
        .collect();
    let highlights_md = if breaking.is_empty() {
        None
    } else {
        // Guard against clobbering an existing upgrade guide for this release. We match on
        // the version-suffix rather than the exact `today`-prefixed filename so a partial
        // run from a previous UTC day (or a maintainer-authored guide dated earlier) is
        // still detected. The single-page release layout would otherwise render two
        // "upgrade guide" cards for the same release.
        if let Some(existing) =
            find_existing_upgrade_guide(&repo_root.join(HIGHLIGHTS_DIR), &new_version)?
        {
            bail!(
                "{} already exists for release {new_version}. Delete it (or move it aside) and re-run.",
                existing.display()
            );
        }
        validate_breaking_anchors(&breaking)?;
        Some(render_upgrade_guide(&today, &new_version, &breaking))
    };

    // Everything valid — commit the writes atomically via .tmp + rename.
    atomic_write(&cue_path, &cue_text)?;
    if let Some(md) = highlights_md {
        if let Err(e) = atomic_write(&highlights_path, &md) {
            // Highlights write failed after CUE succeeded — roll the CUE back so the next
            // attempt doesn't hit the "file already exists" guard.
            drop(fs::remove_file(&cue_path));
            return Err(e);
        }
        success!("Wrote {}", highlights_path.display());
    }

    // Format with `cue fmt` (best-effort: warn but do not fail if cue is missing).
    if let Err(e) = run_cue_fmt(&cue_path) {
        warn!("cue fmt failed (skipping format): {e}");
    }

    success!("Wrote {}", cue_path.display());
    Ok(cue_path)
}

// ---------- Tag / version discovery ----------

/// Set of commit identifiers already recorded in `website/cue/reference/releases/*.cue`.
struct ReleasedIdentifiers {
    shas: std::collections::HashSet<String>,
    pr_numbers: std::collections::HashSet<u64>,
}

/// Scan every existing release CUE file for the `sha:` and `pr_number:`
/// fields inside its `commits:` array and return the union as two sets.
///
/// We extract via simple regexes rather than running `cue export`. The shape
/// of these files is well-defined (auto-generated and `cue fmt`-normalised
/// against `urls.cue`) so this is the cheapest correct option, and avoids a
/// runtime dependency on the `cue` binary just for de-duplication.
fn collect_released_identifiers(releases_dir: &Path) -> Result<ReleasedIdentifiers> {
    let mut out = ReleasedIdentifiers {
        shas: std::collections::HashSet::new(),
        pr_numbers: std::collections::HashSet::new(),
    };
    if !releases_dir.is_dir() {
        return Ok(out);
    }
    let sha_re = Regex::new(r#"sha:[ \t]*"([0-9a-fA-F]{7,64})""#).unwrap();
    let pr_re = Regex::new(r"pr_number:[ \t]*([0-9]+)").unwrap();
    for entry in fs::read_dir(releases_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "cue") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        for caps in sha_re.captures_iter(&text) {
            out.shas.insert(caps[1].to_string());
        }
        for caps in pr_re.captures_iter(&text) {
            if let Ok(n) = caps[1].parse::<u64>() {
                out.pr_numbers.insert(n);
            }
        }
    }
    Ok(out)
}

fn validate_single_bump(last: &Version, new: &Version) -> Result<()> {
    if bump_type(last, new).is_none() {
        bail!(
            "The specified version '{new}' must be a single patch, minor, or major bump from {last}"
        );
    }
    Ok(())
}

/// Returns Some("patch"|"minor"|"major") if `new` is exactly one bump above `last`, else None.
fn bump_type(last: &Version, new: &Version) -> Option<&'static str> {
    if new <= last {
        return None;
    }
    let patch = Version::new(last.major, last.minor, last.patch + 1);
    let minor = Version::new(last.major, last.minor + 1, 0);
    let major = if last.major == 0 {
        Version::new(0, last.minor + 1, 0)
    } else {
        Version::new(last.major + 1, 0, 0)
    };
    if *new == patch {
        Some("patch")
    } else if *new == minor {
        Some("minor")
    } else if *new == major {
        Some("major")
    } else {
        None
    }
}

// ---------- Commit fetching / parsing ----------

#[derive(Debug, Clone)]
struct Commit {
    sha: String,
    author: String,
    date: String,
    description: String,
    r#type: Option<String>,
    breaking_change: bool,
    pr_number: Option<u64>,
    files_count: u64,
    insertions_count: u64,
    deletions_count: u64,
}

impl Commit {
    fn validate(&self) -> Result<()> {
        // The release path *must* refuse to write a release CUE that contains
        // commits whose subject didn't match the conventional-commit format —
        // otherwise a malformed PR title slips silently into the published
        // release notes. The Ruby release flow used a strict (`!`-suffixed)
        // parser at this point for the same reason.
        let Some(t) = self.r#type.as_deref() else {
            bail!(
                "Commit {} ({}) does not match the conventional-commit format \
                 (`type(scope): description (#pr)`); fix the PR title or amend \
                 the commit subject before tagging the release.",
                self.sha,
                self.description
            );
        };
        if !ALLOWED_TYPES.contains(&t) {
            bail!(
                "Commit {} has invalid type '{}'. Allowed types: {:?}",
                self.sha,
                t,
                ALLOWED_TYPES
            );
        }
        Ok(())
    }

    fn render_cue(&self) -> String {
        let pr_number = match self.pr_number {
            Some(n) => n.to_string(),
            None => "null".to_string(),
        };
        let type_json = match &self.r#type {
            Some(t) => serde_json::to_string(t).unwrap(),
            None => "null".to_string(),
        };
        format!(
            "{{sha: {sha}, date: {date}, description: {description}, pr_number: {pr_number}, type: {type_field}, breaking_change: {breaking}, author: {author}, files_count: {files}, insertions_count: {ins}, deletions_count: {del}}}",
            sha = json!(self.sha),
            date = json!(self.date),
            description = json!(self.description),
            type_field = type_json,
            breaking = self.breaking_change,
            author = json!(self.author),
            files = self.files_count,
            ins = self.insertions_count,
            del = self.deletions_count,
        )
    }
}

fn fetch_commits_since(last_version: &Version) -> Result<Vec<Commit>> {
    // Use the three-dot symmetric-difference range so `--cherry-pick`
    // / `--right-only` filter out commits already released on the previous
    // tag's branch (matches the Ruby `v#{last_version}...` form).
    let range = format!("v{last_version}...");
    let log_output = git::run_and_check_output(&[
        "log",
        &range,
        "--cherry-pick",
        "--right-only",
        "--no-merges",
        "--pretty=format:%H\t%s\t%aN\t%aI",
    ])?;

    let mut commits: Vec<Commit> = Vec::new();
    for line in log_output.lines().rev() {
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        if parts.len() != 4 {
            warn!("Skipping unparsable git log line: {line}");
            continue;
        }
        let sha = parts[0].to_string();
        let message = parts[1];
        let author = parts[2].to_string();
        let date = format_commit_date(parts[3]);
        let conv = ConventionalParts::parse(message);
        let (files, ins, del) = commit_stats(&sha)?;

        commits.push(Commit {
            sha,
            author,
            date,
            description: conv.description,
            r#type: conv.r#type,
            breaking_change: conv.breaking_change,
            pr_number: conv.pr_number,
            files_count: files,
            insertions_count: ins,
            deletions_count: del,
        });
    }
    Ok(commits)
}

/// Convert an ISO-8601 commit date (`%aI`) to the "YYYY-MM-DD HH:MM:SS UTC" form
/// used in existing release CUE files.
fn format_commit_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso).map_or_else(
        |_| iso.to_string(),
        |dt| {
            dt.with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string()
        },
    )
}

/// Returns `(files_changed, insertions, deletions)` from `git show --shortstat`.
fn commit_stats(sha: &str) -> Result<(u64, u64, u64)> {
    let out = git::run_and_check_output(&["show", "--shortstat", "--oneline", sha])?;
    let stats_line = out.lines().last().unwrap_or("");
    if !stats_line.contains("file") {
        return Ok((0, 0, 0));
    }
    let mut files = 0u64;
    let mut ins = 0u64;
    let mut del = 0u64;
    for part in stats_line.split(',') {
        let part = part.trim();
        let count: u64 = part
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if part.contains("insertion") {
            ins = count;
        } else if part.contains("deletion") {
            del = count;
        } else if part.contains("file") {
            files = count;
        }
    }
    Ok((files, ins, del))
}

#[derive(Debug)]
struct ConventionalParts {
    r#type: Option<String>,
    breaking_change: bool,
    description: String,
    pr_number: Option<u64>,
}

impl ConventionalParts {
    fn parse(message: &str) -> Self {
        let re = Regex::new(
            r"^(?P<type>[a-z]*)(\([a-zA-Z0-9_, -]*\))?(?P<breaking>!)?: (?P<desc>.*?)( \(#(?P<pr>[0-9]+)\))?$",
        )
        .unwrap();

        if let Some(caps) = re.captures(message) {
            let r#type = caps
                .name("type")
                .map(|m| m.as_str().to_string())
                .filter(|s| !s.is_empty());
            let breaking_change = caps.name("breaking").is_some();
            let description = caps
                .name("desc")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let pr_number = caps.name("pr").and_then(|m| m.as_str().parse::<u64>().ok());
            ConventionalParts {
                r#type,
                breaking_change,
                description,
                pr_number,
            }
        } else {
            ConventionalParts {
                r#type: None,
                breaking_change: false,
                description: message.to_string(),
                pr_number: None,
            }
        }
    }
}

// ---------- Changelog.d processing ----------

#[derive(Debug)]
struct ChangelogEntry {
    /// Mapped CUE type ("chore" | "fix" | "feat" | "enhancement").
    cue_type: String,
    breaking: bool,
    description: String,
    contributors: Vec<String>,
    /// For `*.breaking.md` fragments, the structured upgrade-guide details.
    breaking_details: Option<BreakingDetails>,
}

#[derive(Debug, Clone)]
struct BreakingDetails {
    title: String,
    anchor: String,
    /// Content of the fragment's `## Summary` section — reused in the guide so each
    /// breaking change stands on its own without the reader hunting for the release notes.
    summary: String,
    /// Content of the fragment's `## Migration` section (headers, code fences, etc.).
    migration: String,
}

fn read_changelog_fragments(dir: &Path) -> Result<Vec<ChangelogEntry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("README.md"))
        .collect();
    paths.sort();
    for path in paths {
        let entry = parse_changelog_fragment(&path)?;
        entries.push(entry);
    }
    Ok(entries)
}

fn parse_changelog_fragment(path: &Path) -> Result<ChangelogEntry> {
    let stem = path
        .file_stem()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Bad fragment filename: {}", path.display()))?;
    let parts: Vec<&str> = stem.split('.').collect();
    if parts.len() != 2 {
        bail!(
            "Changelog fragment {} is invalid (filename must be <name>.<type>.md)",
            path.display()
        );
    }
    let fragment_type = parts[1];
    let breaking = fragment_type == "breaking";
    let cue_type = match fragment_type {
        "breaking" => "chore",
        "security" => "security",
        "fix" => "fix",
        "feature" => "feat",
        "enhancement" => "enhancement",
        other => bail!(
            "Changelog fragment {} has unrecognized type '{}'",
            path.display(),
            other
        ),
    };

    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    // Strip the `authors:` trailer first — used by every fragment type.
    let (body, contributors) = split_authors(&raw);

    if breaking {
        let (summary, details) = parse_breaking_body(body)
            .with_context(|| format!("Failed to parse breaking fragment {}", path.display()))?;
        return Ok(ChangelogEntry {
            cue_type: cue_type.to_string(),
            breaking,
            description: summary,
            contributors,
            breaking_details: Some(details),
        });
    }

    Ok(ChangelogEntry {
        cue_type: cue_type.to_string(),
        breaking,
        description: body.trim().to_string(),
        contributors,
        breaking_details: None,
    })
}

/// Split off the trailing `authors: <handles...>` line, returning the body preceding it
/// (as a slice into `raw`) plus the parsed handle list. Works with both LF and CRLF line
/// endings — we locate the marker directly in the original byte stream rather than
/// reconstructing offsets from `str::lines()`.
fn split_authors(raw: &str) -> (&str, Vec<String>) {
    let trimmed = raw.trim_end_matches(['\n', '\r']);
    let (body_end, handles_start) = match trimmed.rfind("\nauthors: ") {
        Some(nl) => (nl, nl + 1),
        None if trimmed.starts_with("authors: ") => (0, 0),
        None => return (raw, Vec::new()),
    };
    let body = raw.get(..body_end).unwrap_or("");
    let tail = raw.get(handles_start..).unwrap_or("");
    let handles_line = tail.split(['\n', '\r']).next().unwrap_or(tail);
    let rest = handles_line
        .strip_prefix("authors: ")
        .unwrap_or(handles_line);
    let contributors = rest.split_whitespace().map(String::from).collect();
    (body, contributors)
}

/// Parse the body of a `*.breaking.md` fragment (H1 title + `## Summary` + `## Migration`).
/// Returns `(summary_markdown, breaking_details)`.
fn parse_breaking_body(body: &str) -> Result<(String, BreakingDetails)> {
    let sections = crate::commands::changelog::parse_breaking_sections(body)?;
    let anchor = sections
        .anchor
        .unwrap_or_else(|| crate::commands::changelog::slugify(&sections.title));

    Ok((
        sections.summary.clone(),
        BreakingDetails {
            title: sections.title,
            anchor,
            summary: sections.summary,
            migration: sections.migration,
        },
    ))
}

/// `git rm` every `*.md` under `changelog.d/` except `README.md`. Called by
/// `release prepare` after a successful `run()` — never by the standalone
/// `release generate-cue` subcommand.
pub(super) fn retire_all_fragments() -> Result<()> {
    let repo_root = paths::find_repo_root()?;
    retire_changelog_fragments(&repo_root.join(CHANGELOG_DIR))
}

fn retire_changelog_fragments(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "md") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("README.md") {
            continue;
        }
        let rel = path.strip_prefix(env::current_dir()?).unwrap_or(&path);
        git::rm(&rel.to_string_lossy())?;
    }
    Ok(())
}

// ---------- CUE rendering ----------

fn render_release_cue(
    version: &Version,
    changelog: &[ChangelogEntry],
    commits: &[Commit],
) -> String {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let changelog_block = render_changelog(changelog);
    let commits_block = commits
        .iter()
        .map(Commit::render_cue)
        .collect::<Vec<_>>()
        .join(",\n    ");

    indoc::formatdoc! {"
        package metadata

        releases: \"{version}\": {{
        \tdate:     \"{date}\"

        \twhats_next: []

        \tchangelog: [
        {changelog_block}
        \t]

        \tcommits: [
            {commits_block}
        \t]
        }}
    "}
}

fn render_changelog(entries: &[ChangelogEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let mut s = String::new();
            s.push_str("\t\t{\n");
            writeln!(s, "\t\t\ttype: {}", json!(e.cue_type)).unwrap();
            if e.breaking {
                s.push_str("\t\t\tbreaking: true\n");
            }
            s.push_str("\t\t\tdescription: #\"\"\"\n");
            for line in e.description.lines() {
                writeln!(s, "\t\t\t\t{line}").unwrap();
            }
            s.push_str("\t\t\t\t\"\"\"#\n");
            if !e.contributors.is_empty() {
                let json_contribs = serde_json::to_string(&e.contributors).unwrap();
                writeln!(s, "\t\t\tcontributors: {json_contribs}").unwrap();
            }
            s.push_str("\t\t}");
            s
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

fn run_cue_fmt(path: &Path) -> Result<()> {
    let status = Command::new("cue").arg("fmt").arg(path).status()?;
    if !status.success() {
        bail!("cue fmt exited with {status}");
    }
    Ok(())
}

// ---------- Upgrade-guide (highlights) rendering ----------

/// Return the first file in `highlights_dir` whose name ends in the version-suffix used
/// by upgrade guides (e.g. `-0-58-0-upgrade-guide.md`), regardless of the date prefix.
fn find_existing_upgrade_guide(
    highlights_dir: &Path,
    version: &Version,
) -> Result<Option<PathBuf>> {
    if !highlights_dir.is_dir() {
        return Ok(None);
    }
    let suffix = format!(
        "-{}-{}-{}-upgrade-guide.md",
        version.major, version.minor, version.patch
    );
    for entry in fs::read_dir(highlights_dir)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(&suffix))
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn upgrade_guide_filename(date: &str, version: &Version) -> String {
    let version_slug = format!("{}-{}-{}", version.major, version.minor, version.patch);
    format!("{date}-{version_slug}-upgrade-guide.md")
}

/// Fail the release if any breaking fragment produced an invalid, empty, or duplicate
/// anchor. Uses the same anchor rules as `vdev check changelog-fragments` (shared through
/// `commands::changelog::is_valid_anchor`) so a fragment that passes CI can't fail here.
fn validate_breaking_anchors(breaking: &[&BreakingDetails]) -> Result<()> {
    let mut seen = std::collections::HashMap::<&str, &str>::new();
    for b in breaking {
        if !crate::commands::changelog::is_valid_anchor(&b.anchor) {
            bail!(
                "breaking fragment '{}' has an invalid anchor '{}'. Add `{{#some-valid-slug}}` after the title.",
                b.title,
                b.anchor,
            );
        }
        if let Some(other) = seen.insert(b.anchor.as_str(), b.title.as_str()) {
            bail!(
                "duplicate upgrade-guide anchor '#{}' shared by breaking fragments '{other}' and '{}'. Override one with `{{#unique-slug}}`.",
                b.anchor,
                b.title,
            );
        }
    }
    Ok(())
}

/// Write `content` to `path` via a `.tmp` sibling then atomic rename. Prevents leaving a
/// partial output behind if the process is killed mid-write.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::write(&tmp, content).with_context(|| format!("Failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("Failed to rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn render_upgrade_guide(date: &str, version: &Version, breaking: &[&BreakingDetails]) -> String {
    let title = format!("{}.{} Upgrade Guide", version.major, version.minor);
    let description = format!("An upgrade guide that addresses breaking changes in {version}");

    // No `authors:` in the frontmatter — this file is auto-generated; a "byline" would
    // misattribute a multi-author guide to a single contributor.
    let mut out = String::new();
    out.push_str("---\n");
    writeln!(out, "date: \"{date}\"").unwrap();
    writeln!(out, "title: \"{title}\"").unwrap();
    writeln!(out, "description: \"{description}\"").unwrap();
    writeln!(out, "release: \"{version}\"").unwrap();
    out.push_str("hide_on_release_notes: false\n");
    out.push_str("badges:\n  type: breaking change\n");
    out.push_str("---\n\n");

    // Each fragment becomes an H2 (`## Title`), with Summary/Migration under it at H3.
    // Fragment authors write sub-headings at H4+ (`#### Old` / `#### New`) per the
    // scaffolder's template so nothing needs to be bumped here — Migration content
    // passes through verbatim.
    //
    // Heading levels matter for the highlights page's Tocbot config
    // (`website/assets/js/below.js`, which indexes h2-h5): fragment titles at H2 show up
    // in the TOC as the top-level entries for the guide.
    for b in breaking {
        writeln!(out, "## {} {{#{}}}\n", b.title, b.anchor).unwrap();
        writeln!(out, "### Summary\n\n{}\n", b.summary).unwrap();
        if b.migration.is_empty() {
            writeln!(out, "### Migration\n").unwrap();
        } else {
            writeln!(out, "### Migration\n\n{}\n", b.migration).unwrap();
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_conventional_basic() {
        let p = ConventionalParts::parse("feat(kafka source): add new metric (#123)");
        assert_eq!(p.r#type.as_deref(), Some("feat"));
        assert!(!p.breaking_change);
        assert_eq!(p.description, "add new metric");
        assert_eq!(p.pr_number, Some(123));
    }

    #[test]
    fn parse_conventional_breaking() {
        let p = ConventionalParts::parse("feat(api)!: drop legacy endpoint (#9)");
        assert_eq!(p.r#type.as_deref(), Some("feat"));
        assert!(p.breaking_change);
        assert_eq!(p.description, "drop legacy endpoint");
        assert_eq!(p.pr_number, Some(9));
    }

    #[test]
    fn parse_conventional_with_scope() {
        // Scopes are accepted in the commit subject but no longer stored.
        let p = ConventionalParts::parse("fix(ARC): tweak retry policy (#456)");
        assert_eq!(p.r#type.as_deref(), Some("fix"));
        assert_eq!(p.description, "tweak retry policy");
        assert_eq!(p.pr_number, Some(456));
    }

    #[test]
    fn scoped_subject_parses_and_validates() {
        // End-to-end: a scoped conventional subject must flow through both
        // parsing and Commit::validate without error after the scope drop.
        let subjects = [
            "feat(kafka source): add new metric (#123)",
            "fix(loki sink): handle empty labels (#9)",
            "chore(deps): bump tokio (#1)",
            "enhancement(api, config): tweak schema (#42)",
        ];
        for subject in subjects {
            let p = ConventionalParts::parse(subject);
            assert!(p.r#type.is_some(), "parse failed for: {subject}");
            let c = Commit {
                sha: "x".into(),
                author: "a".into(),
                date: "d".into(),
                description: p.description,
                r#type: p.r#type,
                breaking_change: p.breaking_change,
                pr_number: p.pr_number,
                files_count: 0,
                insertions_count: 0,
                deletions_count: 0,
            };
            c.validate()
                .unwrap_or_else(|e| panic!("validate failed for {subject}: {e}"));
        }
    }

    #[test]
    fn parse_conventional_unparsable_fallthrough() {
        let p = ConventionalParts::parse("Merge branch 'foo'");
        assert!(p.r#type.is_none());
        assert_eq!(p.description, "Merge branch 'foo'");
    }

    #[test]
    fn bump_type_patch_minor_major() {
        let last = Version::new(1, 2, 3);
        assert_eq!(bump_type(&last, &Version::new(1, 2, 4)), Some("patch"));
        assert_eq!(bump_type(&last, &Version::new(1, 3, 0)), Some("minor"));
        assert_eq!(bump_type(&last, &Version::new(2, 0, 0)), Some("major"));
        assert_eq!(bump_type(&last, &Version::new(1, 2, 5)), None);
        assert_eq!(bump_type(&last, &Version::new(1, 2, 3)), None);
    }

    #[test]
    fn bump_type_zero_major() {
        // For 0.x, "major" bump means 0.(x+1).0
        let last = Version::new(0, 55, 0);
        assert_eq!(bump_type(&last, &Version::new(0, 55, 1)), Some("patch"));
        assert_eq!(bump_type(&last, &Version::new(0, 56, 0)), Some("minor"));
    }

    #[test]
    fn read_changelog_fragments_maps_types_and_authors() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("README.md"), "ignored").unwrap();
        fs::write(
            dir.join("123_my_change.feature.md"),
            indoc::indoc! {"
                Adds a thing.

                Issue: https://example/123

                authors: alice bob
            "},
        )
        .unwrap();
        fs::write(
            dir.join("legacy_break.breaking.md"),
            indoc::indoc! {"
                # Legacy thing removed

                ## Summary

                Removed legacy thing.

                ## Migration

                N/A

                authors: dave
            "},
        )
        .unwrap();
        fs::write(dir.join("sec.security.md"), "Patched a CVE.\n").unwrap();

        let entries = read_changelog_fragments(dir).unwrap();
        assert_eq!(entries.len(), 3);

        // Sorted by filename
        let by_type: Vec<_> = entries.iter().map(|e| e.cue_type.as_str()).collect();
        assert_eq!(by_type, vec!["feat", "chore", "security"]);

        let feat = &entries[0];
        assert_eq!(
            feat.contributors,
            vec!["alice".to_string(), "bob".to_string()]
        );
        assert!(feat.description.starts_with("Adds a thing."));
        assert!(!feat.description.contains("authors:"));

        // Breaking fragments must be marked as such and carry structured details.
        let breaking = &entries[1];
        assert!(breaking.breaking);
        assert!(breaking.breaking_details.is_some());
        let details = breaking.breaking_details.as_ref().unwrap();
        assert_eq!(details.title, "Legacy thing removed");
        assert_eq!(details.anchor, "legacy-thing-removed");
        assert_eq!(details.migration.trim(), "N/A");
        // Breaking description in the CUE is the Summary, not the whole body.
        assert_eq!(breaking.description, "Removed legacy thing.");
        assert_eq!(breaking.contributors, vec!["dave".to_string()]);
        assert!(!entries[0].breaking);
    }

    #[test]
    fn read_changelog_fragments_rejects_unknown_type() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("foo.bogus.md"), "x").unwrap();
        assert!(read_changelog_fragments(tmp.path()).is_err());
    }

    #[test]
    fn render_release_cue_matches_known_shape() {
        let entries = vec![
            ChangelogEntry {
                cue_type: "feat".into(),
                breaking: false,
                description: "Adds a thing.\nMulti-line.".into(),
                contributors: vec!["alice".into()],
                breaking_details: None,
            },
            ChangelogEntry {
                cue_type: "fix".into(),
                breaking: false,
                description: "Fixed it.".into(),
                contributors: vec![],
                breaking_details: None,
            },
        ];
        let commits = vec![Commit {
            sha: "abc123".into(),
            author: "Pavlos".into(),
            date: "2026-05-06 12:00:00 UTC".into(),
            description: "do stuff".into(),
            r#type: Some("feat".into()),
            breaking_change: false,
            pr_number: Some(42),
            files_count: 1,
            insertions_count: 2,
            deletions_count: 3,
        }];

        let out = render_release_cue(&Version::new(0, 99, 0), &entries, &commits);

        assert!(out.starts_with("package metadata\n"));
        assert!(out.contains("releases: \"0.99.0\":"));
        assert!(out.contains("\twhats_next: []\n"));
        assert!(out.contains("\t\t\ttype: \"feat\"\n"));
        assert!(out.contains("\t\t\t\tAdds a thing.\n"));
        assert!(out.contains("\t\t\t\tMulti-line.\n"));
        assert!(out.contains("contributors: [\"alice\"]"));
        assert!(out.contains("\t\t\ttype: \"fix\"\n"));
        assert!(out.contains("sha: \"abc123\""));
        assert!(!out.contains("scopes:"));
        assert!(out.contains("pr_number: 42"));
        assert!(out.contains("files_count: 1"));
    }

    #[test]
    fn commit_validate_scope_is_optional_for_all_types() {
        for t in ALLOWED_TYPES {
            let c = Commit {
                sha: "x".into(),
                author: "a".into(),
                date: "d".into(),
                description: "no scope".into(),
                r#type: Some((*t).into()),
                breaking_change: false,
                pr_number: None,
                files_count: 0,
                insertions_count: 0,
                deletions_count: 0,
            };
            assert!(
                c.validate().is_ok(),
                "type '{t}' should be valid without a scope"
            );
        }
    }

    #[test]
    fn commit_validate_rejects_unknown_type() {
        let c = Commit {
            sha: "x".into(),
            author: "a".into(),
            date: "d".into(),
            description: "x".into(),
            r#type: Some("nope".into()),
            breaking_change: false,
            pr_number: None,
            files_count: 0,
            insertions_count: 0,
            deletions_count: 0,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn commit_validate_rejects_unparsable_subject() {
        // A non-conventional subject must abort the release path rather than
        // silently land in the published CUE with type=null.
        let c = Commit {
            sha: "x".into(),
            author: "a".into(),
            date: "d".into(),
            description: "Merge branch 'foo'".into(),
            r#type: None,
            breaking_change: false,
            pr_number: None,
            files_count: 0,
            insertions_count: 0,
            deletions_count: 0,
        };
        let err = c.validate().expect_err("must reject unparsable subject");
        let msg = format!("{err}");
        assert!(
            msg.contains("conventional-commit format"),
            "error should mention the rule: {msg}"
        );
    }

    #[test]
    fn collect_released_identifiers_extracts_shas_and_pr_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        // A trimmed-down CUE file shaped like the real release cues.
        fs::write(
            tmp.path().join("0.55.0.cue"),
            r#"package metadata

releases: "0.55.0": {
    commits: [
        {sha: "deadbeefcafe", date: "2026-01-01 00:00:00 UTC", pr_number: 42, type: "fix"},
        {sha: "abc1234567890abc", pr_number: 99},
    ]
}
"#,
        )
        .unwrap();
        // A non-cue file we should ignore.
        fs::write(tmp.path().join("README.md"), "not a release").unwrap();

        let ids = collect_released_identifiers(tmp.path()).unwrap();
        assert!(ids.shas.contains("deadbeefcafe"));
        assert!(ids.shas.contains("abc1234567890abc"));
        assert!(ids.pr_numbers.contains(&42));
        assert!(ids.pr_numbers.contains(&99));
    }

    #[test]
    fn collect_released_identifiers_handles_missing_dir() {
        let ids = collect_released_identifiers(Path::new("/nonexistent")).unwrap();
        assert!(ids.shas.is_empty());
        assert!(ids.pr_numbers.is_empty());
    }

    #[test]
    fn split_authors_lf() {
        let raw = "line one\nline two\n\nauthors: alice bob\n";
        let (body, authors) = split_authors(raw);
        assert_eq!(body, "line one\nline two\n");
        assert_eq!(authors, vec!["alice".to_string(), "bob".to_string()]);
    }

    #[test]
    fn split_authors_crlf() {
        // Windows checkouts commit fragments with CRLF endings — the previous impl
        // used `str::lines()` (strips \r) plus a `+1` byte-per-line accumulator,
        // which truncated the body by one byte per line. Verify the whole body
        // survives now.
        let raw = "line one\r\nline two\r\n\r\nauthors: alice bob\r\n";
        let (body, authors) = split_authors(raw);
        assert!(body.contains("line one"));
        assert!(body.contains("line two"));
        assert!(!body.contains("authors:"));
        assert_eq!(authors, vec!["alice".to_string(), "bob".to_string()]);
    }

    #[test]
    fn split_authors_no_authors_line() {
        let raw = "just body text\nmore body\n";
        let (body, authors) = split_authors(raw);
        assert_eq!(body, raw);
        assert!(authors.is_empty());
    }

    #[test]
    fn parse_breaking_body_extracts_summary_and_migration() {
        let body = indoc::indoc! {"
            # Env var interpolation off {#env-var}

            ## Summary

            Off by default now.

            ## Migration

            Pass the flag.

            ```bash
            vector --config vector.yaml
            ```
        "};
        let (summary, details) = parse_breaking_body(body).unwrap();
        assert_eq!(summary, "Off by default now.");
        assert_eq!(details.title, "Env var interpolation off");
        assert_eq!(details.anchor, "env-var");
        assert!(details.migration.starts_with("Pass the flag."));
        assert!(details.migration.contains("```bash"));
    }

    #[test]
    fn parse_breaking_body_derives_anchor_from_title() {
        let body = indoc::indoc! {"
            # A Big Change!

            ## Summary

            x

            ## Migration

            N/A
        "};
        let (_, details) = parse_breaking_body(body).unwrap();
        assert_eq!(details.anchor, "a-big-change");
    }

    #[test]
    fn slugify_examples() {
        assert_eq!(
            crate::commands::changelog::slugify("A Big Change!"),
            "a-big-change"
        );
        assert_eq!(
            crate::commands::changelog::slugify("  --Foo/Bar--  "),
            "foo-bar"
        );
        assert_eq!(
            crate::commands::changelog::slugify("already-good"),
            "already-good"
        );
        assert_eq!(
            crate::commands::changelog::slugify("Numbers 123 OK"),
            "numbers-123-ok"
        );
    }

    #[test]
    fn upgrade_guide_filename_uses_version() {
        let name = upgrade_guide_filename("2026-07-17", &Version::parse("0.58.0").unwrap());
        assert!(name.ends_with("-0-58-0-upgrade-guide.md"), "{name}");
    }

    #[test]
    fn find_existing_upgrade_guide_matches_any_date() {
        let tmp = tempfile::tempdir().unwrap();
        // A guide dated on some past day (simulating a failed run yesterday, or a
        // maintainer-authored guide dated earlier than today).
        fs::write(
            tmp.path().join("2026-07-19-0-58-0-upgrade-guide.md"),
            "---\nrelease: 0.58.0\n---",
        )
        .unwrap();
        // An unrelated highlight for a different release must NOT match.
        fs::write(
            tmp.path().join("2026-07-19-0-57-0-upgrade-guide.md"),
            "---\nrelease: 0.57.0\n---",
        )
        .unwrap();

        let hit =
            find_existing_upgrade_guide(tmp.path(), &Version::parse("0.58.0").unwrap()).unwrap();
        assert!(
            hit.as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("2026-07-19-0-58-0-upgrade-guide.md"),
            "{hit:?}"
        );

        let miss =
            find_existing_upgrade_guide(tmp.path(), &Version::parse("0.59.0").unwrap()).unwrap();
        assert!(miss.is_none(), "{miss:?}");
    }

    #[test]
    fn find_existing_upgrade_guide_handles_missing_dir() {
        let hit = find_existing_upgrade_guide(
            std::path::Path::new("/nonexistent-dir-for-test"),
            &Version::parse("0.58.0").unwrap(),
        )
        .unwrap();
        assert!(hit.is_none());
    }

    fn bd(title: &str, anchor: &str, summary: &str, migration: &str) -> BreakingDetails {
        BreakingDetails {
            title: title.into(),
            anchor: anchor.into(),
            summary: summary.into(),
            migration: migration.into(),
        }
    }

    #[test]
    fn validate_breaking_anchors_rejects_duplicates() {
        let a = bd("First", "same", "", "");
        let b = bd("Second", "same", "", "");
        let err = validate_breaking_anchors(&[&a, &b]).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    #[test]
    fn validate_breaking_anchors_rejects_empty() {
        let a = bd("非ASCII", "", "", "");
        let err = validate_breaking_anchors(&[&a]).unwrap_err();
        assert!(err.to_string().contains("invalid anchor"), "{err}");
    }

    #[test]
    fn validate_breaking_anchors_accepts_uniques() {
        let a = bd("First", "first", "", "");
        let b = bd("Second", "second", "", "");
        validate_breaking_anchors(&[&a, &b]).unwrap();
    }

    #[test]
    fn render_upgrade_guide_shape() {
        let version = Version::parse("0.58.0").unwrap();
        // First fragment models the scaffolder's default template: prose plus
        // `### Old` / `### New` fenced code examples. These sub-headings must pass
        // through into the guide verbatim (no bumping, no rewriting). Uses generic
        // placeholders instead of real Vector flags so the test doesn't churn on
        // unrelated CLI renames.
        let d1 = bd(
            "First breaking change",
            "first",
            "Something changed.",
            // Fragment authors write H4 sub-sections so no bumping is needed in the
            // generator — Migration content passes through verbatim.
            indoc::indoc! {"
                Pass `--new-flag` on startup to restore the previous behavior.

                #### Old

                ```bash
                vector --config vector.yaml
                ```

                #### New

                ```bash
                vector --config vector.yaml --new-flag
                ```"}
            .trim_end(),
        );
        let d2 = bd(
            "Second breaking change",
            "second",
            "A deprecated label is gone.",
            "N/A",
        );
        let md = render_upgrade_guide("2026-07-17", &version, &[&d1, &d2]);
        let expected = indoc::indoc! {r#"
            ---
            date: "2026-07-17"
            title: "0.58 Upgrade Guide"
            description: "An upgrade guide that addresses breaking changes in 0.58.0"
            release: "0.58.0"
            hide_on_release_notes: false
            badges:
              type: breaking change
            ---

            ## First breaking change {#first}

            ### Summary

            Something changed.

            ### Migration

            Pass `--new-flag` on startup to restore the previous behavior.

            #### Old

            ```bash
            vector --config vector.yaml
            ```

            #### New

            ```bash
            vector --config vector.yaml --new-flag
            ```

            ## Second breaking change {#second}

            ### Summary

            A deprecated label is gone.

            ### Migration

            N/A

        "#};
        assert_eq!(md, expected);
    }

    #[test]
    fn render_upgrade_guide_empty_migration() {
        let version = Version::parse("0.58.0").unwrap();
        let d = bd("A change", "a-change", "Something changed.", "");
        let md = render_upgrade_guide("2026-07-17", &version, &[&d]);
        let expected = indoc::indoc! {r#"
            ---
            date: "2026-07-17"
            title: "0.58 Upgrade Guide"
            description: "An upgrade guide that addresses breaking changes in 0.58.0"
            release: "0.58.0"
            hide_on_release_notes: false
            badges:
              type: breaking change
            ---

            ## A change {#a-change}

            ### Summary

            Something changed.

            ### Migration

        "#};
        assert_eq!(md, expected);
    }
}
