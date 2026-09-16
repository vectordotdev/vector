use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use semver::Version;
use serde_json::json;

use crate::commands::changelog::FRAGMENT_TYPES;
use crate::utils::{git, paths};

const RELEASES_DIR: &str = "website/cue/reference/releases";
const CHANGELOG_DIR: &str = "changelog.d";
const HIGHLIGHTS_DIR: &str = "website/content/en/highlights";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PullRequestMetadata {
    Optional,
    Required,
}

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
        run(&self.version, PullRequestMetadata::Optional)?;
        Ok(())
    }
}

/// Generate the release CUE file for the given new version. Returns the path that was written.
///
/// Pure generation: does not touch `changelog.d/`. Callers that want the fragments retired
/// after a successful release run should call [`retire_all_fragments`] afterward.
pub(super) fn run(
    new_version: &Version,
    pull_request_metadata: PullRequestMetadata,
) -> Result<PathBuf> {
    let repo_root = paths::find_repo_root()?;
    env::set_current_dir(&repo_root)?;

    info!("Creating release meta file...");

    let last_version = git::latest_release_version()?;

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
    let stub_path = repo_root
        .join("website")
        .join("content")
        .join("en")
        .join("releases")
        .join(format!("{new_version}.md"));
    if stub_path.exists() {
        bail!(
            "{} already exists. Delete it (or move it aside) and re-run.",
            stub_path.display()
        );
    }
    // Note: the highlights collision is checked later, only if we're actually going to
    // write it — so a manually-authored upgrade guide doesn't block a release with no
    // breaking fragments.

    let changelog_dir = repo_root.join(CHANGELOG_DIR);
    let changelog_entries =
        read_changelog_fragments(&repo_root, &changelog_dir, pull_request_metadata)?;

    // Validate + render everything IN MEMORY before touching disk, so a validation
    // failure doesn't leave a partial CUE file behind (which would then trip the
    // "file already exists" guard on the next run).
    let cue_text = render_release_cue(&new_version, &changelog_entries);
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

    refresh_versions_cue(&repo_root)?;
    write_release_stub(&repo_root, &new_version)?;

    success!("Wrote {}", cue_path.display());
    Ok(cue_path)
}

/// Regenerate `website/cue/reference/versions.cue` from the filenames in the `releases/`
/// directory, preserving any versions already listed there that have no backing `.cue` file
/// (legacy releases predating the automated tooling). Fixes the gap where `release generate-cue`
/// previously left `versions.cue` stale so the new version was invisible in local Hugo previews.
pub(super) fn refresh_versions_cue(repo_root: &Path) -> Result<()> {
    let releases_dir = repo_root.join(RELEASES_DIR);
    let mut versions: std::collections::HashSet<Version> = fs::read_dir(&releases_dir)
        .with_context(|| format!("Failed to read {}", releases_dir.display()))?
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            let p = e.path();
            if p.extension().is_some_and(|ext| ext == "cue") {
                p.file_stem()?.to_str()?.parse::<Version>().ok()
            } else {
                None
            }
        })
        .collect();

    // Preserve versions already in versions.cue that have no backing CUE file — these are
    // legacy releases that predate the per-release CUE files and must not be silently dropped.
    let versions_cue_path = repo_root
        .join("website")
        .join("cue")
        .join("reference")
        .join("versions.cue");
    if let Ok(text) = fs::read_to_string(&versions_cue_path) {
        for line in text.lines() {
            let trimmed = line.trim().trim_end_matches(',').trim_matches('"');
            if let Ok(v) = trimmed.parse::<Version>() {
                versions.insert(v);
            }
        }
    }

    let mut versions: Vec<Version> = versions.into_iter().collect();
    versions.sort_by(|a, b| b.cmp(a));

    let list = versions
        .iter()
        .map(|v| format!("\t\"{v}\","))
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!("package metadata\n\nversions: [string, ...string] & [\n{list}\n]\n");

    let versions_cue = repo_root
        .join("website")
        .join("cue")
        .join("reference")
        .join("versions.cue");
    atomic_write(&versions_cue, &content)?;
    Ok(())
}

/// Write `website/content/en/releases/<version>.md` — the Hugo stub Hugo needs to route
/// `/releases/<version>/`. Weight is derived from the highest weight found among existing
/// stubs so the new release naturally sorts last (newest).
pub(super) fn write_release_stub(repo_root: &Path, version: &Version) -> Result<()> {
    let releases_dir = repo_root
        .join("website")
        .join("content")
        .join("en")
        .join("releases");

    let stub_path = releases_dir.join(format!("{version}.md"));
    if stub_path.exists() {
        bail!(
            "{} already exists. Delete it (or move it aside) and re-run.",
            stub_path.display()
        );
    }

    let max_weight: u32 = fs::read_dir(&releases_dir)
        .with_context(|| format!("Failed to read {}", releases_dir.display()))?
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|x| x == "md")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n != "_index.md")
        })
        .filter_map(|e| {
            let text = fs::read_to_string(e.path()).ok()?;
            text.lines()
                .find(|l| l.trim_start().starts_with("weight:"))?
                .split_once(':')?
                .1
                .trim()
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);

    let content = format!(
        "---\ntitle: Vector v{version} release notes\nweight: {}\n---\n",
        max_weight + 1
    );
    atomic_write(&stub_path, &content)?;
    Ok(())
}

// ---------- Tag / version discovery ----------

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

// ---------- Changelog.d processing ----------

#[derive(Debug)]
struct ChangelogEntry {
    /// Mapped CUE type ("chore" | "fix" | "feat" | "enhancement").
    cue_type: String,
    breaking: bool,
    description: String,
    pr_numbers: Vec<u64>,
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

fn read_changelog_fragments(
    repo_root: &Path,
    dir: &Path,
    pull_request_metadata: PullRequestMetadata,
) -> Result<Vec<ChangelogEntry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    #[cfg(not(test))]
    if pull_request_metadata == PullRequestMetadata::Required {
        ensure_full_git_history(repo_root)?;
    }

    let mut entries = Vec::new();
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("README.md"))
        .collect();
    paths.sort();
    for path in paths {
        let mut entry = parse_changelog_fragment(&path)?;
        entry.pr_numbers = lookup_pull_requests(repo_root, &path, pull_request_metadata)?;
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
    let Some(entry) = FRAGMENT_TYPES.iter().find(|t| t.name == fragment_type) else {
        bail!(
            "Changelog fragment {} has unrecognized type '{}' (valid types: {})",
            path.display(),
            fragment_type,
            FRAGMENT_TYPES
                .iter()
                .map(|t| t.name)
                .collect::<Vec<_>>()
                .join("|")
        );
    };
    let breaking = entry.breaking;
    let cue_type = entry.cue_type;

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
            pr_numbers: Vec::new(),
            contributors,
            breaking_details: Some(details),
        });
    }

    Ok(ChangelogEntry {
        cue_type: cue_type.to_string(),
        breaking,
        description: body.trim().to_string(),
        pr_numbers: Vec::new(),
        contributors,
        breaking_details: None,
    })
}

/// Find every PR that added or edited the current lifetime of a changelog fragment
/// from each commit's `... (#12345)` title. Deletion commits are excluded so this
/// also works after release preparation removes the fragment.
#[cfg(not(test))]
fn lookup_pull_requests(
    repo_root: &Path,
    fragment_path: &Path,
    pull_request_metadata: PullRequestMetadata,
) -> Result<Vec<u64>> {
    lookup_pull_requests_from_git(repo_root, fragment_path, pull_request_metadata)
}

fn lookup_pull_requests_from_git(
    repo_root: &Path,
    fragment_path: &Path,
    pull_request_metadata: PullRequestMetadata,
) -> Result<Vec<u64>> {
    let relative_path = fragment_path.strip_prefix(repo_root).with_context(|| {
        format!(
            "Fragment path {} is outside the repository root {}",
            fragment_path.display(),
            repo_root.display()
        )
    })?;
    let relative_path = relative_path.to_str().ok_or_else(|| {
        anyhow!(
            "Fragment path is not valid UTF-8: {}",
            fragment_path.display()
        )
    })?;

    let addition_commits = run_command(
        "git",
        &[
            "log",
            "--format=%H",
            "--diff-filter=A",
            "--follow",
            "--",
            relative_path,
        ],
        repo_root,
    )?;
    let Some(latest_addition) = addition_commits.lines().next() else {
        return match pull_request_metadata {
            PullRequestMetadata::Optional => Ok(Vec::new()),
            PullRequestMetadata::Required => bail!(
                "Could not find the commit that added {relative_path}; cannot determine its pull requests."
            ),
        };
    };

    let commit_history = run_command(
        "git",
        &[
            "log",
            "--format=%H%x09%s",
            "--diff-filter=AMR",
            "--follow",
            "--",
            relative_path,
        ],
        repo_root,
    )?;

    parse_pull_request_history(&commit_history, latest_addition, pull_request_metadata)
        .with_context(|| {
            format!("Could not determine every PR that added or edited {relative_path}")
        })
}

fn parse_pull_request_history(
    commit_history: &str,
    latest_addition: &str,
    pull_request_metadata: PullRequestMetadata,
) -> Result<Vec<u64>> {
    let mut numbers = Vec::new();
    for line in commit_history
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let (commit, title) = line
            .split_once('\t')
            .ok_or_else(|| anyhow!("Malformed git log entry `{line}`"))?;

        match parse_pull_request_number(title) {
            Ok(number) if !numbers.contains(&number) => numbers.push(number),
            Ok(_) => {}
            Err(_) if pull_request_metadata == PullRequestMetadata::Optional => {}
            Err(error) => return Err(error.context(format!("Commit {commit}: `{title}`"))),
        }

        if commit == latest_addition {
            return Ok(numbers);
        }
    }

    match pull_request_metadata {
        PullRequestMetadata::Optional => Ok(Vec::new()),
        PullRequestMetadata::Required => {
            bail!("Could not find latest addition commit {latest_addition} in the fragment history")
        }
    }
}

fn parse_pull_request_number(commit_title: &str) -> Result<u64> {
    let number = commit_title
        .trim()
        .strip_suffix(')')
        .and_then(|title| title.rsplit_once("(#"))
        .map(|(_, number)| number)
        .filter(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| anyhow!("Commit title must end with `(#<PR number>)`"))?;

    number
        .parse()
        .with_context(|| format!("Invalid PR number in commit title `{commit_title}`"))
}

#[cfg(not(test))]
fn ensure_full_git_history(repo_root: &Path) -> Result<()> {
    let is_shallow = run_command("git", &["rev-parse", "--is-shallow-repository"], repo_root)?;
    if is_shallow.trim() == "true" {
        bail!(
            "Release generation requires full Git history to find the PRs that introduced changelog fragments."
        );
    }
    Ok(())
}

#[cfg(test)]
fn lookup_pull_requests(_: &Path, _: &Path, _: PullRequestMetadata) -> Result<Vec<u64>> {
    // Unit tests exercise local parsing and rendering without requiring a Git repository.
    Ok(vec![42])
}

fn run_command(cmd: &str, args: &[&str], cwd: &Path) -> Result<String> {
    let display = format!("{cmd} {}", args.join(" "));
    let output = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("Failed to run `{display}`"))?;

    if !output.status.success() {
        bail!(
            "`{display}` failed (exit {}):\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

fn render_release_cue(version: &Version, changelog: &[ChangelogEntry]) -> String {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let changelog_block = render_changelog(changelog);

    indoc::formatdoc! {"
        package metadata

        releases: \"{version}\": {{
        \tdate:     \"{date}\"

        \tchangelog: [
        {changelog_block}
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
                if let Some(details) = &e.breaking_details {
                    writeln!(s, "\t\t\ttitle: {}", json!(details.title)).unwrap();
                    writeln!(s, "\t\t\tanchor: {}", json!(details.anchor)).unwrap();
                }
            }
            s.push_str("\t\t\tdescription: #\"\"\"\n");
            for line in e.description.lines() {
                writeln!(s, "\t\t\t\t{line}").unwrap();
            }
            s.push_str("\t\t\t\t\"\"\"#\n");
            if !e.pr_numbers.is_empty() {
                let json_prs = serde_json::to_string(&e.pr_numbers).unwrap();
                writeln!(s, "\t\t\tpr_numbers: {json_prs}").unwrap();
            }
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
    fn parses_pull_request_from_commit_title() {
        assert_eq!(
            parse_pull_request_number("feat(foo): add bar (#12345)").unwrap(),
            12345
        );
    }

    #[test]
    fn parses_and_deduplicates_pull_requests_from_commit_titles() {
        let history = indoc::indoc! {"
            c3	fix(foo): adjust bar (#23456)
            c2	feat(foo): add bar (#12345)
            c1	feat(foo): add bar (#12345)
        "};
        assert_eq!(
            parse_pull_request_history(history, "c1", PullRequestMetadata::Required).unwrap(),
            vec![23456, 12345]
        );
    }

    #[test]
    fn pull_request_history_stops_at_latest_addition() {
        let history = indoc::indoc! {"
            current-edit	fix(foo): adjust bar (#300)
            current-add	feat(foo): add bar (#298)
            old-edit	fix(foo): old adjustment (#120)
            old-add	feat(foo): old addition (#119)
        "};

        assert_eq!(
            parse_pull_request_history(history, "current-add", PullRequestMetadata::Required,)
                .unwrap(),
            vec![300, 298]
        );
    }

    #[test]
    fn pull_request_lookup_includes_renamed_fragment_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let changelog_dir = repo.join("changelog.d");
        fs::create_dir(&changelog_dir).unwrap();

        run_command("git", &["init", "--quiet"], repo).unwrap();
        run_command("git", &["config", "core.hooksPath", "/dev/null"], repo).unwrap();
        run_command("git", &["config", "user.name", "Vector Test"], repo).unwrap();
        run_command("git", &["config", "user.email", "vector@example.com"], repo).unwrap();
        run_command("git", &["config", "commit.gpgsign", "false"], repo).unwrap();

        let original = changelog_dir.join("original.enhancement.md");
        let renamed = changelog_dir.join("renamed.enhancement.md");
        fs::write(
            &original,
            "Original entry.\nIt documents the first behavior.\nIt includes migration details.\n",
        )
        .unwrap();
        run_command("git", &["add", "changelog.d/original.enhancement.md"], repo).unwrap();
        run_command(
            "git",
            &["commit", "--quiet", "-m", "feat(foo): add entry (#100)"],
            repo,
        )
        .unwrap();

        run_command(
            "git",
            &[
                "mv",
                "changelog.d/original.enhancement.md",
                "changelog.d/renamed.enhancement.md",
            ],
            repo,
        )
        .unwrap();
        fs::write(
            &renamed,
            "Original entry.\nIt documents the first behavior.\nIt includes migration details.\nOne more detail.\n",
        )
        .unwrap();
        run_command("git", &["add", "changelog.d/renamed.enhancement.md"], repo).unwrap();
        run_command(
            "git",
            &[
                "commit",
                "--quiet",
                "-m",
                "fix(foo): rename and edit entry (#101)",
            ],
            repo,
        )
        .unwrap();

        assert_eq!(
            lookup_pull_requests_from_git(repo, &renamed, PullRequestMetadata::Required).unwrap(),
            vec![101, 100]
        );
    }

    #[test]
    fn optional_pull_request_metadata_skips_unmerged_commits() {
        let history = indoc::indoc! {"
            edit	fix(foo): adjust bar
            add	feat(foo): add bar (#12345)
        "};

        assert_eq!(
            parse_pull_request_history(history, "add", PullRequestMetadata::Optional).unwrap(),
            vec![12345]
        );
        assert!(parse_pull_request_history(history, "add", PullRequestMetadata::Required).is_err());
    }

    #[test]
    fn optional_pull_request_metadata_tolerates_missing_history() {
        assert_eq!(
            parse_pull_request_history("", "missing", PullRequestMetadata::Optional).unwrap(),
            Vec::<u64>::new()
        );
        assert!(parse_pull_request_history("", "missing", PullRequestMetadata::Required).is_err());
    }

    #[test]
    fn rejects_commit_title_without_pull_request_suffix() {
        assert!(parse_pull_request_number("feat(foo): add bar").is_err());
        assert!(parse_pull_request_number("feat(foo): add bar (#abc)").is_err());
        assert!(parse_pull_request_number("feat(foo): add bar (#123) trailing").is_err());
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

        let entries = read_changelog_fragments(dir, dir, PullRequestMetadata::Optional).unwrap();
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
        assert_eq!(feat.pr_numbers, vec![42]);

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
        assert!(
            read_changelog_fragments(tmp.path(), tmp.path(), PullRequestMetadata::Optional)
                .is_err()
        );
    }

    #[test]
    fn render_release_cue_matches_known_shape() {
        let entries = vec![
            ChangelogEntry {
                cue_type: "feat".into(),
                breaking: false,
                description: "Adds a thing.\nMulti-line.".into(),
                pr_numbers: vec![123],
                contributors: vec!["alice".into()],
                breaking_details: None,
            },
            ChangelogEntry {
                cue_type: "fix".into(),
                breaking: false,
                description: "Fixed it.".into(),
                pr_numbers: vec![],
                contributors: vec![],
                breaking_details: None,
            },
            ChangelogEntry {
                cue_type: "chore".into(),
                breaking: true,
                description: "Removed legacy thing.".into(),
                pr_numbers: vec![456],
                contributors: vec![],
                breaking_details: Some(BreakingDetails {
                    title: "Legacy thing removed".into(),
                    anchor: "legacy-thing-removed".into(),
                    summary: "Removed legacy thing.".into(),
                    migration: "N/A".into(),
                }),
            },
        ];
        let out = render_release_cue(&Version::new(0, 99, 0), &entries);

        assert!(out.starts_with("package metadata\n"));
        assert!(out.contains("releases: \"0.99.0\":"));
        assert!(out.contains("\t\t\ttype: \"feat\"\n"));
        assert!(out.contains("\t\t\t\tAdds a thing.\n"));
        assert!(out.contains("\t\t\t\tMulti-line.\n"));
        assert!(out.contains("contributors: [\"alice\"]"));
        assert!(out.contains("pr_numbers: [123]"));
        assert!(out.contains("pr_numbers: [456]"));
        assert!(out.contains("\t\t\ttype: \"fix\"\n"));
        assert!(out.contains("\t\t\ttype: \"chore\"\n"));
        assert!(out.contains("\t\t\tbreaking: true\n"));
        assert!(out.contains("\t\t\ttitle: \"Legacy thing removed\"\n"));
        assert!(out.contains("\t\t\tanchor: \"legacy-thing-removed\"\n"));
        assert!(!out.contains("commits:"));
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

    #[test]
    fn refresh_versions_cue_sorts_descending_and_writes_correct_format() {
        let tmp = tempfile::tempdir().unwrap();
        let releases_dir = tmp.path().join("website/cue/reference/releases");
        fs::create_dir_all(&releases_dir).unwrap();
        for v in ["0.9.0", "0.10.0", "0.9.1", "0.8.2"] {
            fs::write(releases_dir.join(format!("{v}.cue")), "").unwrap();
        }
        fs::write(releases_dir.join("README.md"), "not a version").unwrap();

        refresh_versions_cue(tmp.path()).unwrap();

        let out =
            fs::read_to_string(tmp.path().join("website/cue/reference/versions.cue")).unwrap();

        let expected = indoc::indoc! {r#"
            package metadata

            versions: [string, ...string] & [
            	"0.10.0",
            	"0.9.1",
            	"0.9.0",
            	"0.8.2",
            ]
        "#};
        assert_eq!(out, expected);
    }

    #[test]
    fn write_release_stub_picks_max_weight_and_writes_correct_format() {
        let tmp = tempfile::tempdir().unwrap();
        let stubs_dir = tmp.path().join("website/content/en/releases");
        fs::create_dir_all(&stubs_dir).unwrap();
        fs::write(stubs_dir.join("_index.md"), "---\n---\n").unwrap();
        fs::write(
            stubs_dir.join("0.56.0.md"),
            "---\ntitle: Vector v0.56.0 release notes\nweight: 35\n---\n",
        )
        .unwrap();
        fs::write(
            stubs_dir.join("0.57.0.md"),
            "---\ntitle: Vector v0.57.0 release notes\nweight: 36\n---\n",
        )
        .unwrap();

        let version = Version::parse("0.58.0").unwrap();
        write_release_stub(tmp.path(), &version).unwrap();

        let out = fs::read_to_string(stubs_dir.join("0.58.0.md")).unwrap();
        assert_eq!(
            out,
            "---\ntitle: Vector v0.58.0 release notes\nweight: 37\n---\n"
        );
    }

    #[test]
    fn write_release_stub_rejects_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let stubs_dir = tmp.path().join("website/content/en/releases");
        fs::create_dir_all(&stubs_dir).unwrap();
        fs::write(
            stubs_dir.join("0.58.0.md"),
            "---\ntitle: Vector v0.58.0 release notes\nweight: 37\n---\n",
        )
        .unwrap();

        let version = Version::parse("0.58.0").unwrap();
        let err = write_release_stub(tmp.path(), &version).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
    }

    #[test]
    fn refresh_versions_cue_preserves_legacy_versions_without_cue_files() {
        let tmp = tempfile::tempdir().unwrap();
        let releases_dir = tmp.path().join("website/cue/reference/releases");
        fs::create_dir_all(&releases_dir).unwrap();
        fs::write(releases_dir.join("0.15.0.cue"), "").unwrap();

        // Simulate an existing versions.cue that lists a legacy version with no CUE file.
        let versions_cue_dir = tmp.path().join("website/cue/reference");
        let existing = indoc::indoc! {r#"
            package metadata

            versions: [string, ...string] & [
            	"0.15.0",
            	"0.14.1",
            ]
        "#};
        fs::write(versions_cue_dir.join("versions.cue"), existing).unwrap();

        refresh_versions_cue(tmp.path()).unwrap();

        let out = fs::read_to_string(versions_cue_dir.join("versions.cue")).unwrap();
        assert!(out.contains("\"0.15.0\""), "CUE-backed version missing");
        assert!(out.contains("\"0.14.1\""), "legacy version was dropped");
        // 0.15.0 must sort above 0.14.1
        assert!(
            out.find("\"0.15.0\"") < out.find("\"0.14.1\""),
            "wrong sort order"
        );
    }
}
