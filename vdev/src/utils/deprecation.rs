use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
pub const DEPRECATION_DIR: &str = "deprecation.d";
pub const DEPRECATIONS_JSON: &str = "website/data/deprecations.json";

/// A concrete semver version identifying when a deprecation was announced.
/// Accepted forms: `"0.56"` (major.minor) or `"0.56.0"` (major.minor.patch).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeprecationVersion(pub Version);

impl fmt::Display for DeprecationVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl DeprecationVersion {
    /// Returns true when this version's major.minor matches the release.
    /// Patch is ignored so `0.56` (stored as 0.56.0) matches any 0.56.x release.
    pub fn matches_release(&self, release: &Version) -> bool {
        self.0.major == release.major && self.0.minor == release.minor
    }
}

impl<'de> serde::Deserialize<'de> for DeprecationVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let s = s.trim();
        // Accept both "0.56" (major.minor) and "0.56.0" (major.minor.patch).
        // Normalize the two-part form by appending ".0".
        let normalized = if s.chars().filter(|&c| c == '.').count() == 1 {
            std::borrow::Cow::Owned(format!("{s}.0"))
        } else {
            std::borrow::Cow::Borrowed(s)
        };
        let v = Version::parse(&normalized)
            .map_err(|e| serde::de::Error::custom(format!("invalid version '{s}': {e}")))?;
        if !v.pre.is_empty() || !v.build.is_empty() {
            return Err(serde::de::Error::custom(format!(
                "invalid version '{s}': prerelease and build metadata are not allowed; use plain X.Y or X.Y.Z"
            )));
        }
        Ok(DeprecationVersion(v))
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Frontmatter {
    what: String,
    deprecated_since: DeprecationVersion,
}

/// A parsed and validated deprecation entry from `deprecation.d/`.
#[derive(Debug, Clone)]
pub struct DeprecationEntry {
    pub filename: String,
    pub what: String,
    /// The release in which this deprecation was first announced.
    pub deprecated_since: DeprecationVersion,
    /// Optional body text (everything after the closing `---` of the frontmatter).
    pub description: String,
}

/// The result of partitioning deprecation entries relative to a specific release.
pub struct DeprecationPartition {
    /// Entries whose `deprecated_since` matches the release (announced for the first time now).
    pub announcing: Vec<DeprecationEntry>,
    /// Entries whose `deprecated_since` predates the release (announced in an earlier release).
    pub planned: Vec<DeprecationEntry>,
    /// Entries whose `deprecated_since` post-dates the release (announced in a future release).
    pub future: Vec<DeprecationEntry>,
}

/// Partition a list of deprecation entries into three buckets relative to `release`.
pub fn partition_by_release(
    entries: Vec<DeprecationEntry>,
    release: &Version,
) -> DeprecationPartition {
    let mut announcing = Vec::new();
    let mut planned = Vec::new();
    let mut future = Vec::new();
    for e in entries {
        if e.deprecated_since.matches_release(release) {
            announcing.push(e);
        } else if e.deprecated_since.0 < *release {
            planned.push(e);
        } else {
            future.push(e);
        }
    }
    DeprecationPartition {
        announcing,
        planned,
        future,
    }
}

/// Read and parse all deprecation fragments from the given directory.
/// Returns entries sorted by filename.
pub fn read_deprecation_fragments(dir: &Path) -> Result<Vec<DeprecationEntry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_deprecation_fragment(p))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| parse_deprecation_fragment(&p))
        .collect()
}

fn is_deprecation_fragment(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name == "README.md" {
        return false;
    }
    if !std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return false;
    }
    // Reject symlinks and non-regular files so a `.md -> /dev/zero` or
    // out-of-tree symlink can't hang or exfiltrate.
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

fn parse_deprecation_fragment(path: &Path) -> Result<DeprecationEntry> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    let (frontmatter_str, body) = split_frontmatter(&raw, path)?;

    let fm: Frontmatter = serde_yaml::from_str(frontmatter_str)
        .with_context(|| format!("Failed to parse YAML frontmatter in {}", path.display()))?;

    if fm.what.trim().is_empty() {
        bail!(
            "Deprecation fragment {}: `what` field must not be empty",
            path.display()
        );
    }

    Ok(DeprecationEntry {
        filename,
        what: fm.what.trim().to_string(),
        deprecated_since: fm.deprecated_since,
        description: body.trim().to_string(),
    })
}

/// Split the raw file contents into the frontmatter string and the body.
/// The file must begin with `---`, and have a closing `---` on its own line.
fn split_frontmatter<'a>(content: &'a str, path: &Path) -> Result<(&'a str, &'a str)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        bail!(
            "Deprecation fragment {} must begin with YAML frontmatter (---)",
            path.display()
        );
    }

    // Advance past the opening `---` (and optional trailing whitespace on that line)
    let after_open = content[3..].trim_start_matches([' ', '\t']);
    let after_open = after_open.trim_start_matches('\n');

    let close_pos = after_open.find("\n---").ok_or_else(|| {
        anyhow!(
            "Deprecation fragment {} has unclosed frontmatter",
            path.display()
        )
    })?;

    let frontmatter = &after_open[..close_pos];
    let rest = &after_open[close_pos + 4..]; // skip `\n---`
    let body = rest.trim_start_matches(['\r', '\n']);

    Ok((frontmatter, body))
}

/// A deprecation that has been enacted (feature removed).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnactedEntry {
    pub what: String,
    pub deprecated_since: String,
    pub removed_in: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DeprecationsJson {
    deprecations_pending: Vec<PendingJsonEntry>,
    deprecations_enacted: Vec<EnactedEntry>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PendingJsonEntry {
    what: String,
    deprecated_since: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
}

/// Read only the enacted entries from `DEPRECATIONS_JSON`.
pub fn read_enacted(repo_root: &Path) -> Result<Vec<EnactedEntry>> {
    Ok(read_json(repo_root)?.deprecations_enacted)
}

fn read_json(repo_root: &Path) -> Result<DeprecationsJson> {
    let path = repo_root.join(DEPRECATIONS_JSON);
    if !path.exists() {
        return Ok(DeprecationsJson {
            deprecations_pending: Vec::new(),
            deprecations_enacted: Vec::new(),
        });
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("Failed to parse {}", path.display()))
}

fn write_json(repo_root: &Path, data: &DeprecationsJson) -> Result<()> {
    let path = repo_root.join(DEPRECATIONS_JSON);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let out = serde_json::to_string_pretty(data)? + "\n";
    fs::write(&path, out).with_context(|| format!("Failed to write {}", path.display()))
}

/// Append an enacted entry and regenerate the pending section from deprecation.d/.
///
/// Idempotent on an exact duplicate `(what, removed_in)`: silently skips the
/// append so that re-running `enact` after a partial failure (e.g. the
/// fragment delete failed) can complete cleanly. A conflicting record with
/// the same `what` but a different `removed_in` is still rejected because
/// the same feature cannot be removed twice.
pub fn append_enacted(repo_root: &Path, entry: EnactedEntry) -> Result<()> {
    let dir = repo_root.join(DEPRECATION_DIR);
    let pending = read_deprecation_fragments(&dir)?;
    let mut data = read_json(repo_root)?;
    if let Some(existing) = data
        .deprecations_enacted
        .iter()
        .find(|e| e.what == entry.what)
    {
        if existing.removed_in != entry.removed_in {
            bail!(
                "Conflicting enacted entry for '{}': already recorded as removed in {}, refusing to record as removed in {}",
                entry.what,
                existing.removed_in,
                entry.removed_in
            );
        }
        // Exact duplicate; rewrite pending so a partial failure can recover,
        // but don't push the entry again.
    } else {
        data.deprecations_enacted.push(entry);
    }
    data.deprecations_pending = pending_excluding_enacted(&pending, &data.deprecations_enacted);
    write_json(repo_root, &data)
}

/// Filter pending fragments so they exclude any `what` already in `enacted`.
/// Keeps the JSON consistent if `enact` was interrupted between the JSON
/// write and the fragment delete: the next `generate` / `check` will still
/// produce a JSON without the enacted-and-still-on-disk fragment in
/// `deprecations_pending`.
fn pending_excluding_enacted(
    pending: &[DeprecationEntry],
    enacted: &[EnactedEntry],
) -> Vec<PendingJsonEntry> {
    let enacted_what: std::collections::HashSet<&str> =
        enacted.iter().map(|e| e.what.as_str()).collect();
    pending
        .iter()
        .filter(|e| !enacted_what.contains(e.what.as_str()))
        .map(|e| PendingJsonEntry {
            what: e.what.clone(),
            deprecated_since: e.deprecated_since.to_string(),
            description: e.description.clone(),
        })
        .collect()
}

/// Validate the enacted entries in `DEPRECATIONS_JSON`:
///   * both versions must be valid semver
///   * `removed_in` must be in a later major.minor than `deprecated_since`
///     (matching the deprecation policy and the `enact` command's rule)
///   * no duplicate `what` values (a feature can't be removed twice, even
///     in different releases)
pub fn validate_enacted(repo_root: &Path) -> Result<usize> {
    let enacted = read_enacted(repo_root)?;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in &enacted {
        let dep = Version::parse(&e.deprecated_since).with_context(|| {
            format!(
                "Enacted entry '{}' has invalid deprecated_since '{}'",
                e.what, e.deprecated_since
            )
        })?;
        let rem = Version::parse(&e.removed_in).with_context(|| {
            format!(
                "Enacted entry '{}' has invalid removed_in '{}'",
                e.what, e.removed_in
            )
        })?;
        if !later_minor(&rem, &dep) {
            bail!(
                "Enacted entry '{}' has removed_in ({}) that is not in a later minor release than deprecated_since ({}); the deprecation policy requires at least one minor release between announcement and removal.",
                e.what,
                e.removed_in,
                e.deprecated_since
            );
        }
        if !seen.insert(e.what.clone()) {
            bail!(
                "Duplicate enacted entry for '{}'; the same feature cannot be recorded as removed more than once.",
                e.what
            );
        }
    }
    Ok(enacted.len())
}

/// True when `a` is in a later major.minor release than `b`. Equal or smaller
/// major.minor (regardless of patch) returns false.
pub fn later_minor(a: &Version, b: &Version) -> bool {
    (a.major, a.minor) > (b.major, b.minor)
}

/// Render the JSON that `sync_deprecations_cue` would write, without
/// touching disk. Returns the serialized string (with trailing newline) so
/// callers can compare against the on-disk file.
pub fn rendered_json(repo_root: &Path) -> Result<String> {
    let dir = repo_root.join(DEPRECATION_DIR);
    let pending = read_deprecation_fragments(&dir)?;
    let mut data = read_json(repo_root)?;
    data.deprecations_pending = pending_excluding_enacted(&pending, &data.deprecations_enacted);
    Ok(serde_json::to_string_pretty(&data)? + "\n")
}

/// Regenerate `DEPRECATIONS_JSON` from `deprecation.d/` (pending) plus the
/// existing enacted entries already in the file.
pub fn sync_deprecations_cue(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(DEPRECATION_DIR);
    let pending = read_deprecation_fragments(&dir)?;
    let mut data = read_json(repo_root)?;
    data.deprecations_pending = pending_excluding_enacted(&pending, &data.deprecations_enacted);
    write_json(repo_root, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parse_full_entry() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("foo_opt.md"),
            "---\nwhat: The foo option\ndeprecated_since: \"0.57.0\"\n---\n\nDetailed explanation.\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.what, "The foo option");
        assert_eq!(
            e.deprecated_since,
            DeprecationVersion(Version::new(0, 57, 0))
        );
        assert_eq!(e.description, "Detailed explanation.");
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("bad.md"), "No frontmatter here.\n").unwrap();
        assert!(read_deprecation_fragments(tmp.path()).is_err());
    }

    #[test]
    fn rejects_empty_what() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("empty.md"),
            "---\nwhat: \"   \"\ndeprecated_since: \"0.60.0\"\n---\n",
        )
        .unwrap();
        assert!(read_deprecation_fragments(tmp.path()).is_err());
    }

    #[test]
    fn skips_readme() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("README.md"), "# ignored").unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_two_part_version() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("short.md"),
            "---\nwhat: Short version\ndeprecated_since: \"0.56\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(
            entries[0].deprecated_since,
            DeprecationVersion(Version::new(0, 56, 0))
        );
    }

    #[test]
    fn matches_release_ignores_patch() {
        let v = DeprecationVersion(Version::new(0, 56, 0));
        assert!(v.matches_release(&Version::new(0, 56, 0)));
        assert!(v.matches_release(&Version::new(0, 56, 1)));
        assert!(!v.matches_release(&Version::new(0, 57, 0)));
    }

    #[test]
    fn partition_two_buckets() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("new.md"),
            "---\nwhat: New announcement\ndeprecated_since: \"0.56\"\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("old.md"),
            "---\nwhat: Previously announced\ndeprecated_since: \"0.53\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        let p = partition_by_release(entries, &Version::new(0, 56, 0));
        assert_eq!(p.announcing.len(), 1);
        assert_eq!(p.announcing[0].what, "New announcement");
        assert_eq!(p.planned.len(), 1);
        assert_eq!(p.planned[0].what, "Previously announced");
        assert!(p.future.is_empty());
    }

    #[test]
    fn partition_separates_future_from_planned() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("future.md"),
            "---\nwhat: Future plan\ndeprecated_since: \"0.99\"\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("old.md"),
            "---\nwhat: Old plan\ndeprecated_since: \"0.10\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        let p = partition_by_release(entries, &Version::new(0, 56, 0));
        assert!(p.announcing.is_empty());
        assert_eq!(p.planned.len(), 1);
        assert_eq!(p.planned[0].what, "Old plan");
        assert_eq!(p.future.len(), 1);
        assert_eq!(p.future[0].what, "Future plan");
    }

    #[test]
    fn rejects_symlinked_fragments() {
        // A symlink to /dev/zero (or anything outside the tree) must not be
        // ingested as a fragment. We assert behavior by symlinking inside the
        // tempdir; the loader should still refuse to read the link.
        let tmp = tempdir().unwrap();
        // Real fragment that should still be picked up.
        fs::write(
            tmp.path().join("real.md"),
            "---\nwhat: real\ndeprecated_since: \"0.56\"\n---\n",
        )
        .unwrap();
        let target = tmp.path().join("real.md");
        let link = tmp.path().join("link.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &link).unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        // Only the real file is included; the symlink is silently dropped.
        let filenames: Vec<&str> = entries.iter().map(|e| e.filename.as_str()).collect();
        assert_eq!(filenames, vec!["real.md"]);
    }

    #[test]
    fn append_enacted_is_idempotent_on_exact_duplicate() {
        // Re-running `enact` after a partial failure should succeed without
        // duplicating the enacted entry.
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("deprecation.d")).unwrap();
        fs::create_dir_all(tmp.path().join("website/data")).unwrap();
        let entry = EnactedEntry {
            what: "foo".into(),
            deprecated_since: "0.55.0".into(),
            removed_in: "0.56.0".into(),
            description: String::new(),
        };
        append_enacted(tmp.path(), entry.clone()).unwrap();
        append_enacted(tmp.path(), entry).unwrap();
        let enacted = read_enacted(tmp.path()).unwrap();
        assert_eq!(enacted.len(), 1);
    }

    #[test]
    fn append_enacted_rejects_conflicting_removed_in() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("deprecation.d")).unwrap();
        fs::create_dir_all(tmp.path().join("website/data")).unwrap();
        let entry = EnactedEntry {
            what: "foo".into(),
            deprecated_since: "0.55.0".into(),
            removed_in: "0.56.0".into(),
            description: String::new(),
        };
        append_enacted(tmp.path(), entry.clone()).unwrap();
        let conflict = EnactedEntry {
            removed_in: "0.57.0".into(),
            ..entry
        };
        let err = append_enacted(tmp.path(), conflict).unwrap_err();
        assert!(format!("{err}").contains("Conflicting"));
    }

    #[test]
    fn validate_enacted_catches_bad_versions_and_duplicates() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("website/data")).unwrap();
        // Invalid removed_in.
        fs::write(
            tmp.path().join("website/data/deprecations.json"),
            r#"{"deprecations_pending":[],"deprecations_enacted":[
                {"what":"x","deprecated_since":"0.55.0","removed_in":"not-a-version"}
            ]}"#,
        )
        .unwrap();
        assert!(validate_enacted(tmp.path()).is_err());

        // Duplicate `what` (different removed_in) — still rejected.
        fs::write(
            tmp.path().join("website/data/deprecations.json"),
            r#"{"deprecations_pending":[],"deprecations_enacted":[
                {"what":"x","deprecated_since":"0.55.0","removed_in":"0.56.0"},
                {"what":"x","deprecated_since":"0.55.0","removed_in":"0.57.0"}
            ]}"#,
        )
        .unwrap();
        let err = validate_enacted(tmp.path()).unwrap_err();
        assert!(format!("{err}").contains("Duplicate"));
    }

    #[test]
    fn validate_enacted_rejects_same_minor() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("website/data")).unwrap();
        // removed_in is later semver (patch bump) but same minor as
        // deprecated_since — policy violation that `enact` rejects, so a
        // hand-edited JSON should also fail validation.
        fs::write(
            tmp.path().join("website/data/deprecations.json"),
            r#"{"deprecations_pending":[],"deprecations_enacted":[
                {"what":"x","deprecated_since":"0.57.0","removed_in":"0.57.1"}
            ]}"#,
        )
        .unwrap();
        let err = validate_enacted(tmp.path()).unwrap_err();
        assert!(format!("{err}").contains("later minor release"));
    }

    #[test]
    fn rendered_json_drops_pending_already_enacted() {
        // Simulate an interrupted enact: the fragment file is still on disk
        // (delete failed) but the JSON already has the matching enacted entry.
        // rendered_json must keep deprecations_pending consistent with
        // deprecations_enacted so `check` doesn't accept the inconsistent
        // intermediate state.
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("deprecation.d");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(tmp.path().join("website/data")).unwrap();
        fs::write(
            dir.join("foo.md"),
            "---\nwhat: foo\ndeprecated_since: \"0.55.0\"\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("website/data/deprecations.json"),
            r#"{"deprecations_pending":[],"deprecations_enacted":[
                {"what":"foo","deprecated_since":"0.55.0","removed_in":"0.57.0"}
            ]}"#,
        )
        .unwrap();
        let out = rendered_json(tmp.path()).unwrap();
        assert!(out.contains("\"deprecations_pending\": []"));
        assert!(out.contains("\"what\": \"foo\""));
    }

    #[test]
    fn rejects_prerelease_and_build_metadata() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("pre.md"),
            "---\nwhat: foo\ndeprecated_since: \"0.58.0-alpha\"\n---\n",
        )
        .unwrap();
        let err = read_deprecation_fragments(tmp.path()).unwrap_err();
        assert!(format!("{err:?}").contains("prerelease and build metadata"));

        let tmp2 = tempdir().unwrap();
        fs::write(
            tmp2.path().join("build.md"),
            "---\nwhat: foo\ndeprecated_since: \"0.58.0+ci\"\n---\n",
        )
        .unwrap();
        let err = read_deprecation_fragments(tmp2.path()).unwrap_err();
        assert!(format!("{err:?}").contains("prerelease and build metadata"));
    }

    #[test]
    fn later_minor_semantics() {
        assert!(later_minor(
            &Version::new(0, 58, 0),
            &Version::new(0, 57, 0)
        ));
        assert!(!later_minor(
            &Version::new(0, 57, 1),
            &Version::new(0, 57, 0)
        ));
        assert!(!later_minor(
            &Version::new(0, 57, 0),
            &Version::new(0, 57, 0)
        ));
        assert!(!later_minor(
            &Version::new(0, 56, 5),
            &Version::new(0, 57, 0)
        ));
    }
}
