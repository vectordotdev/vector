use std::{
    fmt,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;

pub const DEPRECATION_DIR: &str = "deprecation.d";

/// A version field that is either a concrete semver version or the placeholder `TBD`.
#[derive(Debug, Clone, PartialEq)]
pub enum VersionOrTbd {
    Version(Version),
    Tbd,
}

impl fmt::Display for VersionOrTbd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionOrTbd::Version(v) => write!(f, "{v}"),
            VersionOrTbd::Tbd => write!(f, "TBD"),
        }
    }
}

impl VersionOrTbd {
    /// Returns true when this version's major.minor matches `release`'s major.minor.
    /// Patch is intentionally ignored: a deprecation labelled "0.56" (stored as 0.56.0)
    /// is enacted on any 0.56.x release cut.
    pub fn matches_release(&self, release: &Version) -> bool {
        match self {
            VersionOrTbd::Version(v) => v.major == release.major && v.minor == release.minor,
            VersionOrTbd::Tbd => false,
        }
    }
}

impl<'de> serde::Deserialize<'de> for VersionOrTbd {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let s = s.trim();
        if s == "TBD" {
            return Ok(VersionOrTbd::Tbd);
        }
        // Accept both "0.56" (major.minor) and "0.56.0" (major.minor.patch).
        // Normalize the two-part form by appending ".0".
        let normalized = if s.chars().filter(|&c| c == '.').count() == 1 {
            std::borrow::Cow::Owned(format!("{s}.0"))
        } else {
            std::borrow::Cow::Borrowed(s)
        };
        Version::parse(&normalized)
            .map(VersionOrTbd::Version)
            .map_err(|e| serde::de::Error::custom(format!("invalid version '{s}': {e}")))
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Frontmatter {
    what: String,
    deprecation_version: VersionOrTbd,
    announcement_version: Option<VersionOrTbd>,
}

/// A parsed and validated deprecation entry from `deprecation.d/`.
#[derive(Debug, Clone)]
pub struct DeprecationEntry {
    pub filename: String,
    pub what: String,
    pub deprecation_version: VersionOrTbd,
    pub announcement_version: Option<VersionOrTbd>,
    /// Optional body text (everything after the closing `---` of the frontmatter).
    pub description: String,
}

/// Read and parse all deprecation fragments from the given directory.
/// Returns entries sorted by filename.
pub fn read_deprecation_fragments(dir: &Path) -> Result<Vec<DeprecationEntry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("README.md"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| parse_deprecation_fragment(&p))
        .collect()
}

fn parse_deprecation_fragment(path: &Path) -> Result<DeprecationEntry> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    if !filename.ends_with(".md") {
        bail!(
            "Deprecation fragment {} must have a .md extension",
            path.display()
        );
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    let (frontmatter_str, body) = split_frontmatter(&raw, path)?;

    let fm: Frontmatter = serde_yaml::from_str(frontmatter_str).with_context(|| {
        format!(
            "Failed to parse YAML frontmatter in {}",
            path.display()
        )
    })?;

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
        announcement_version: fm.announcement_version,
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

    let close_pos = after_open
        .find("\n---")
        .ok_or_else(|| anyhow!("Deprecation fragment {} has unclosed frontmatter", path.display()))?;

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
    fn parse_full_entry() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("foo_opt.md"),
            "---\nwhat: The foo option\ndeprecation_version: \"0.57.0\"\nannouncement_version: \"0.55.0\"\n---\n\nDetailed explanation.\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.what, "The foo option");
        assert_eq!(e.deprecation_version, VersionOrTbd::Version(Version::new(0, 57, 0)));
        assert_eq!(
            e.announcement_version,
            Some(VersionOrTbd::Version(Version::new(0, 55, 0)))
        );
        assert_eq!(e.description, "Detailed explanation.");
    }

    #[test]
    fn parse_tbd_versions() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("bar.md"),
            "---\nwhat: Bar thing\ndeprecation_version: \"TBD\"\nannouncement_version: \"TBD\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert_eq!(entries[0].deprecation_version, VersionOrTbd::Tbd);
        assert_eq!(entries[0].announcement_version, Some(VersionOrTbd::Tbd));
    }

    #[test]
    fn parse_no_announcement_version() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("baz.md"),
            "---\nwhat: Baz option\ndeprecation_version: \"0.60.0\"\n---\n",
        )
        .unwrap();
        let entries = read_deprecation_fragments(tmp.path()).unwrap();
        assert!(entries[0].announcement_version.is_none());
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
        // "0.56" normalises to 0.56.0
        assert_eq!(
            entries[0].deprecation_version,
            VersionOrTbd::Version(Version::new(0, 56, 0))
        );
    }

    #[test]
    fn matches_release_ignores_patch() {
        let v = VersionOrTbd::Version(Version::new(0, 56, 0));
        // "0.56" (stored as 0.56.0) should match any 0.56.x release
        assert!(v.matches_release(&Version::new(0, 56, 0)));
        assert!(v.matches_release(&Version::new(0, 56, 1)));
        assert!(v.matches_release(&Version::new(0, 56, 99)));
        // Different minor/major must not match
        assert!(!v.matches_release(&Version::new(0, 57, 0)));
        assert!(!v.matches_release(&Version::new(1, 56, 0)));
    }

    #[test]
    fn tbd_never_matches_release() {
        assert!(!VersionOrTbd::Tbd.matches_release(&Version::new(0, 56, 0)));
    }
}
