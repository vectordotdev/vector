use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
/// Active deprecation notices.
pub const DEPRECATION_DIR: &str = "deprecation.d";
/// Enacted (removed) deprecation notices; same format plus `removed_in`.
pub const ENACTED_DIR: &str = "website/content/en/deprecations/enacted";

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
    #[serde(default)]
    removed_in: Option<DeprecationVersion>,
}

/// A parsed and validated deprecation entry from `deprecation.d/` or `ENACTED_DIR`.
#[derive(Debug, Clone)]
pub struct DeprecationEntry {
    pub filename: String,
    pub what: String,
    /// The release in which this deprecation was first announced.
    pub deprecated_since: DeprecationVersion,
    /// The release in which the deprecated feature was removed. Set only for enacted entries.
    pub removed_in: Option<DeprecationVersion>,
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
///
/// Rejects:
///   * the directory itself being a symlink (treated as missing)
///   * duplicate `what` values across fragments
pub fn read_deprecation_fragments(dir: &Path) -> Result<Vec<DeprecationEntry>> {
    // Use symlink_metadata so a symlinked `deprecation.d` doesn't pull in
    // out-of-tree content.
    let Ok(meta) = fs::symlink_metadata(dir) else {
        return Ok(Vec::new());
    };
    if !meta.file_type().is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_deprecation_fragment(p))
        .collect();
    paths.sort();
    let entries: Vec<DeprecationEntry> = paths
        .into_iter()
        .map(|p| parse_deprecation_fragment(&p))
        .collect::<Result<_>>()?;

    // Reject duplicate `what` across pending fragments. A feature has one
    // identity; two fragments with the same `what` would publish the same
    // deprecation twice.
    let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for e in &entries {
        if let Some(prev) = seen.insert(e.what.as_str(), e.filename.as_str()) {
            bail!(
                "Duplicate deprecation fragments for `what`: '{}' (in {} and {})",
                e.what,
                prev,
                e.filename
            );
        }
    }
    Ok(entries)
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
        removed_in: fm.removed_in,
        description: body.trim().to_string(),
    })
}

/// Split the raw file contents into the frontmatter string and the body.
/// The file must begin with `---`, and have a closing `---` on its own line.
fn split_frontmatter<'a>(content: &'a str, path: &Path) -> Result<(&'a str, &'a str)> {
    let content = content.trim_start();
    // Advance past the opening `---` (and optional trailing whitespace on that line)
    let after_open = content
        .strip_prefix("---")
        .ok_or_else(|| {
            anyhow!(
                "Deprecation fragment {} must begin with YAML frontmatter (---)",
                path.display()
            )
        })?
        .trim_start_matches([' ', '\t'])
        .trim_start_matches('\n');

    let (frontmatter, rest) = after_open.split_once("\n---").ok_or_else(|| {
        anyhow!(
            "Deprecation fragment {} has unclosed frontmatter",
            path.display()
        )
    })?;
    let rest = rest.trim_start_matches(['\r', '\n']);
    let body = rest.trim_start_matches(['\r', '\n']);

    Ok((frontmatter, body))
}

/// All deprecation notices in the repository.
pub struct Deprecations {
    /// Fragments in `DEPRECATION_DIR`.
    pub active: Vec<DeprecationEntry>,
    /// Fragments in `ENACTED_DIR`.
    pub enacted: Vec<DeprecationEntry>,
}

/// Read and validate the active and enacted deprecation notices.
///
/// Rejects, in addition to the per-directory checks of `read_deprecation_fragments`:
///   * active fragments that set `removed_in`
///   * enacted fragments without `removed_in`, or whose `removed_in` is not in a
///     later minor release than `deprecated_since` (the deprecation policy rule)
///   * the same `what` in both directories (e.g. an interrupted `enact`)
pub fn read_all(repo_root: &Path) -> Result<Deprecations> {
    let active = read_deprecation_fragments(&repo_root.join(DEPRECATION_DIR))?;
    let enacted = read_deprecation_fragments(&repo_root.join(ENACTED_DIR))?;

    for e in &active {
        if let Some(removed_in) = &e.removed_in {
            bail!(
                "{DEPRECATION_DIR}/{} sets removed_in ({removed_in}); \
                 run `cargo vdev deprecation enact` instead of setting it by hand.",
                e.filename
            );
        }
    }
    for e in &enacted {
        let Some(removed_in) = &e.removed_in else {
            bail!("{ENACTED_DIR}/{} is missing removed_in.", e.filename);
        };
        if !later_minor(&removed_in.0, &e.deprecated_since.0) {
            bail!(
                "{ENACTED_DIR}/{} has removed_in ({removed_in}) that is not in a later minor release than deprecated_since ({}); the deprecation policy requires at least one minor release between announcement and removal.",
                e.filename,
                e.deprecated_since
            );
        }
    }

    let enacted_what: std::collections::HashMap<&str, &str> = enacted
        .iter()
        .map(|e| (e.what.as_str(), e.filename.as_str()))
        .collect();
    for e in &active {
        if let Some(prev) = enacted_what.get(e.what.as_str()) {
            bail!(
                "Deprecation '{}' is both active ({DEPRECATION_DIR}/{}) and enacted ({ENACTED_DIR}/{prev}); delete the active fragment.",
                e.what,
                e.filename
            );
        }
    }

    Ok(Deprecations { active, enacted })
}

/// Enact the active fragment `filename`: move it to `ENACTED_DIR` with
/// `removed_in` added to its frontmatter. The body is kept verbatim.
/// Returns the path of the new file.
pub fn enact(repo_root: &Path, filename: &str, removed_in: &Version) -> Result<PathBuf> {
    if !removed_in.pre.is_empty() || !removed_in.build.is_empty() {
        bail!(
            "removed_in {removed_in} has prerelease or build metadata; only plain X.Y.Z is allowed"
        );
    }
    let deprecations = read_all(repo_root)?;
    let src = repo_root.join(DEPRECATION_DIR).join(filename);
    let entry = deprecations
        .active
        .iter()
        .find(|e| e.filename == filename)
        .ok_or_else(|| anyhow!("No deprecation fragment found at {}", src.display()))?;

    if !later_minor(removed_in, &entry.deprecated_since.0) {
        bail!(
            "removed_in ({removed_in}) must be in a later minor release than deprecated_since ({}); \
             the deprecation policy requires at least one minor release between \
             the announcement and removal.",
            entry.deprecated_since
        );
    }

    let dest = repo_root.join(ENACTED_DIR).join(filename);
    if dest.exists() {
        bail!("{} already exists", dest.display());
    }

    let raw =
        fs::read_to_string(&src).with_context(|| format!("Failed to read {}", src.display()))?;
    let (frontmatter, body) = split_frontmatter(&raw, &src)?;
    let mut out = format!(
        "---\n{}\nremoved_in: \"{removed_in}\"\n---\n",
        frontmatter.trim_end()
    );
    let body = body.trim_end();
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }

    fs::create_dir_all(repo_root.join(ENACTED_DIR))?;
    fs::write(&dest, out).with_context(|| format!("Failed to write {}", dest.display()))?;
    fs::remove_file(&src).with_context(|| format!("Failed to delete {}", src.display()))?;
    Ok(dest)
}

/// True when `a` is in a later major.minor release than `b`. Equal or smaller
/// major.minor (regardless of patch) returns false.
pub fn later_minor(a: &Version, b: &Version) -> bool {
    (a.major, a.minor) > (b.major, b.minor)
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

    fn write_fragment(root: &Path, dir: &str, name: &str, content: &str) {
        let dir = root.join(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn read_all_rejects_removed_in_on_active_fragment() {
        let tmp = tempdir().unwrap();
        write_fragment(
            tmp.path(),
            DEPRECATION_DIR,
            "foo.md",
            "---\nwhat: foo\ndeprecated_since: \"0.55.0\"\nremoved_in: \"0.56.0\"\n---\n",
        );
        let err = read_all(tmp.path()).err().unwrap();
        assert!(format!("{err:#}").contains("sets removed_in"));
    }

    #[test]
    fn read_all_validates_removed_in_on_enacted_fragment() {
        for (removed_in, expected) in [
            ("", "missing removed_in"),
            // Patch bump of deprecated_since: same minor, policy violation.
            ("removed_in: \"0.55.1\"\n", "later minor release"),
            (
                "removed_in: \"0.56.0-alpha\"\n",
                "prerelease and build metadata",
            ),
        ] {
            let tmp = tempdir().unwrap();
            write_fragment(
                tmp.path(),
                ENACTED_DIR,
                "foo.md",
                &format!("---\nwhat: foo\ndeprecated_since: \"0.55.0\"\n{removed_in}---\n"),
            );
            let err = read_all(tmp.path()).err().unwrap();
            assert!(
                format!("{err:#}").contains(expected),
                "{removed_in:?}: {err:#}"
            );
        }
    }

    #[test]
    fn read_all_rejects_same_what_active_and_enacted() {
        // State left behind by an `enact` that wrote the enacted file but
        // failed to delete the active fragment.
        let tmp = tempdir().unwrap();
        write_fragment(
            tmp.path(),
            DEPRECATION_DIR,
            "foo.md",
            "---\nwhat: foo\ndeprecated_since: \"0.55.0\"\n---\n",
        );
        write_fragment(
            tmp.path(),
            ENACTED_DIR,
            "foo-renamed.md",
            "---\nwhat: foo\ndeprecated_since: \"0.55.0\"\nremoved_in: \"0.57.0\"\n---\n",
        );
        let err = read_all(tmp.path()).err().unwrap();
        assert!(format!("{err:#}").contains("both active"));
    }

    #[test]
    fn enact_moves_fragment_and_records_removed_in() {
        let tmp = tempdir().unwrap();
        write_fragment(
            tmp.path(),
            DEPRECATION_DIR,
            "foo.md",
            "---\nwhat: \"`foo` option\"\n# kept\ndeprecated_since: \"0.55\"\n---\n\nUse `bar`.\n\n```yaml\nbar: true\n```\n",
        );
        let dest = enact(tmp.path(), "foo.md", &Version::new(0, 57, 0)).unwrap();

        assert!(!tmp.path().join(DEPRECATION_DIR).join("foo.md").exists());
        assert_eq!(dest, tmp.path().join(ENACTED_DIR).join("foo.md"));
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            "---\nwhat: \"`foo` option\"\n# kept\ndeprecated_since: \"0.55\"\nremoved_in: \"0.57.0\"\n---\n\nUse `bar`.\n\n```yaml\nbar: true\n```\n",
        );
        let all = read_all(tmp.path()).unwrap();
        assert!(all.active.is_empty());
        assert_eq!(
            all.enacted[0].removed_in,
            Some(DeprecationVersion(Version::new(0, 57, 0)))
        );
    }

    #[test]
    fn enact_rejects_same_minor_and_keeps_fragment() {
        let tmp = tempdir().unwrap();
        write_fragment(
            tmp.path(),
            DEPRECATION_DIR,
            "foo.md",
            "---\nwhat: foo\ndeprecated_since: \"0.55.0\"\n---\n",
        );
        let err = enact(tmp.path(), "foo.md", &Version::new(0, 55, 1)).unwrap_err();
        assert!(format!("{err}").contains("later minor release"));
        assert!(tmp.path().join(DEPRECATION_DIR).join("foo.md").exists());
        assert!(!tmp.path().join(ENACTED_DIR).exists());
    }

    #[test]
    fn rejects_duplicate_what_across_fragments() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            "---\nwhat: \"the foo option\"\ndeprecated_since: \"0.55.0\"\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.md"),
            "---\nwhat: \"the foo option\"\ndeprecated_since: \"0.56.0\"\n---\n",
        )
        .unwrap();
        let err = read_deprecation_fragments(tmp.path()).unwrap_err();
        assert!(format!("{err}").contains("Duplicate"));
    }

    #[test]
    fn rejects_symlinked_dir() {
        use tempfile::tempdir;
        let outside = tempdir().unwrap();
        fs::write(
            outside.path().join("evil.md"),
            "---\nwhat: \"evil\"\ndeprecated_since: \"0.99.0\"\n---\n",
        )
        .unwrap();

        let host = tempdir().unwrap();
        let link = host.path().join("deprecation.d");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside.path(), &link).unwrap();

        // The symlinked dir must be treated as missing, not read through.
        let entries = read_deprecation_fragments(&link).unwrap();
        assert!(entries.is_empty());
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
