use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;

pub const DEPRECATION_DIR: &str = "deprecation.d";

/// A concrete semver version identifying when a deprecation takes effect.
/// Accepted forms: `"0.56"` (major.minor) or `"0.56.0"` (major.minor.patch).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeprecationVersion(pub Version);

impl fmt::Display for DeprecationVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl DeprecationVersion {
    /// Returns true when this version should be enacted for the given release.
    /// Patch is ignored so `0.56` (stored as 0.56.0) matches any 0.56.x release.
    pub fn matches_release(&self, release: &Version) -> bool {
        self.0.major == release.major && self.0.minor == release.minor
    }

    /// Returns true when this version is strictly in the future relative to `latest`.
    /// A fragment is outdated if its deprecation_version ≤ the latest release.
    pub fn is_future_relative_to(&self, latest: &Version) -> bool {
        (self.0.major, self.0.minor) > (latest.major, latest.minor)
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
        Version::parse(&normalized)
            .map(DeprecationVersion)
            .map_err(|e| serde::de::Error::custom(format!("invalid version '{s}': {e}")))
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Frontmatter {
    what: String,
    deprecation_version: DeprecationVersion,
}

/// A parsed and validated deprecation entry from `deprecation.d/`.
#[derive(Debug, Clone)]
pub struct DeprecationEntry {
    pub filename: String,
    pub what: String,
    pub deprecation_version: DeprecationVersion,
    /// Optional body text (everything after the closing `---` of the frontmatter).
    pub description: String,
    /// True for `*.announced.md` files (announced in a prior release).
    /// False for `*.md` files (being announced for the first time).
    pub previously_announced: bool,
}

/// The result of partitioning deprecation entries relative to a specific release.
pub struct DeprecationPartition {
    /// Entries whose `deprecation_version` matches the release (being removed now).
    pub enacted: Vec<DeprecationEntry>,
    /// Not-enacted entries with `previously_announced = false` (new this release).
    pub announcing: Vec<DeprecationEntry>,
    /// Not-enacted entries with `previously_announced = true` (announced earlier).
    pub planned: Vec<DeprecationEntry>,
}

/// Partition a list of deprecation entries into three buckets relative to `release`.
pub fn partition_by_release(
    entries: Vec<DeprecationEntry>,
    release: &Version,
) -> DeprecationPartition {
    let mut enacted = Vec::new();
    let mut announcing = Vec::new();
    let mut planned = Vec::new();
    for e in entries {
        if e.deprecation_version.matches_release(release) {
            enacted.push(e);
        } else if e.previously_announced {
            planned.push(e);
        } else {
            announcing.push(e);
        }
    }
    DeprecationPartition {
        enacted,
        announcing,
        planned,
    }
}

/// Read and parse all deprecation fragments from the given directory.
/// Includes both `*.md` (new) and `*.announced.md` (previously announced) files.
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

/// Returns the path with `.md` replaced by `.announced.md`.
/// Used by the release tooling to mark a new-announcement fragment as having been announced.
pub fn announced_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let announced_name = format!("{stem}.announced.md");
    path.with_file_name(announced_name)
}

fn is_deprecation_fragment(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if name == "README.md" {
        return false;
    }
    // Accept *.announced.md and *.md (but not *.announced.md double-counted via extension)
    name.ends_with(".announced.md") || (name.ends_with(".md") && !name.ends_with(".announced.md"))
}

fn parse_deprecation_fragment(path: &Path) -> Result<DeprecationEntry> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let previously_announced = filename.ends_with(".announced.md");

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
        deprecation_version: fm.deprecation_version,
        description: body.trim().to_string(),
        previously_announced,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parse_new_entry() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("foo_opt.md"),
            "---\nwhat: The foo option\ndeprecation_version: \"0.57.0\"\n---\n\nDetailed explanation.\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.what, "The foo option");
        assert_eq!(
            e.deprecation_version,
            DeprecationVersion(Version::new(0, 57, 0))
        );
        assert_eq!(e.description, "Detailed explanation.");
        assert!(!e.previously_announced);
    }

    #[test]
    fn parse_announced_entry() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("foo_opt.announced.md"),
            "---\nwhat: The foo option\ndeprecation_version: \"0.57.0\"\n---\n\nDetailed explanation.\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].previously_announced);
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
            "---\nwhat: \"   \"\ndeprecation_version: \"0.60.0\"\n---\n",
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
            "---\nwhat: Short version\ndeprecation_version: \"0.56\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(
            entries[0].deprecation_version,
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
    fn is_future_relative_to() {
        let v = DeprecationVersion(Version::new(0, 57, 0));
        assert!(v.is_future_relative_to(&Version::new(0, 56, 0)));
        assert!(!v.is_future_relative_to(&Version::new(0, 57, 0)));
        assert!(!v.is_future_relative_to(&Version::new(0, 58, 0)));
    }

    #[test]
    fn partition_three_buckets() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("enacted.md"),
            "---\nwhat: Enacted\ndeprecation_version: \"0.56\"\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("new.md"),
            "---\nwhat: New announcement\ndeprecation_version: \"0.58\"\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("old.announced.md"),
            "---\nwhat: Previously announced\ndeprecation_version: \"0.60\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        let p = partition_by_release(entries, &Version::new(0, 56, 0));
        assert_eq!(p.enacted.len(), 1);
        assert_eq!(p.enacted[0].what, "Enacted");
        assert_eq!(p.announcing.len(), 1);
        assert_eq!(p.announcing[0].what, "New announcement");
        assert_eq!(p.planned.len(), 1);
        assert_eq!(p.planned[0].what, "Previously announced");
    }

    #[test]
    fn announced_path_replaces_extension() {
        let p = Path::new("deprecation.d/foo-bar.md");
        assert_eq!(
            announced_path(p),
            Path::new("deprecation.d/foo-bar.announced.md")
        );
    }
}
