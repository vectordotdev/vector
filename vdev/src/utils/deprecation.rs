use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use semver::Version;
use serde_json::json;

pub const DEPRECATION_DIR: &str = "deprecation.d";
pub const ENACTED_JSON: &str = "website/cue/reference/deprecations_enacted.json";
pub const DEPRECATIONS_CUE: &str = "website/cue/reference/deprecations.cue";

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
        Version::parse(&normalized)
            .map(DeprecationVersion)
            .map_err(|e| serde::de::Error::custom(format!("invalid version '{s}': {e}")))
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
}

/// Partition a list of deprecation entries into two buckets relative to `release`.
pub fn partition_by_release(
    entries: Vec<DeprecationEntry>,
    release: &Version,
) -> DeprecationPartition {
    let mut announcing = Vec::new();
    let mut planned = Vec::new();
    for e in entries {
        if e.deprecated_since.matches_release(release) {
            announcing.push(e);
        } else {
            planned.push(e);
        }
    }
    DeprecationPartition { announcing, planned }
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
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    name != "README.md" && name.ends_with(".md")
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

/// Read the list of enacted deprecations from the JSON sidecar file.
pub fn read_enacted(repo_root: &Path) -> Result<Vec<EnactedEntry>> {
    let path = repo_root.join(ENACTED_JSON);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Append an enacted entry to the JSON sidecar file.
pub fn append_enacted(repo_root: &Path, entry: EnactedEntry) -> Result<()> {
    let mut entries = read_enacted(repo_root)?;
    entries.push(entry);
    let path = repo_root.join(ENACTED_JSON);
    let json = serde_json::to_string_pretty(&entries)? + "\n";
    fs::write(&path, json)
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Regenerate `website/cue/reference/deprecations.cue` from the current
/// `deprecation.d/` fragments (pending) and the enacted JSON sidecar.
pub fn sync_deprecations_cue(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(DEPRECATION_DIR);
    let pending = read_deprecation_fragments(&dir)?;
    let enacted = read_enacted(repo_root)?;
    let cue = render_deprecations_cue(&pending, &enacted);
    let path = repo_root.join(DEPRECATIONS_CUE);
    fs::write(&path, cue)
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Public alias used by the check command to verify the CUE file is in sync.
pub fn render_deprecations_cue_for_check(
    pending: &[DeprecationEntry],
    enacted: &[EnactedEntry],
) -> String {
    render_deprecations_cue(pending, enacted)
}

fn render_deprecations_cue(pending: &[DeprecationEntry], enacted: &[EnactedEntry]) -> String {
    let pending_block = render_pending_block(pending);
    let enacted_block = render_enacted_block(enacted);
    format!(
        "// Code generated by `cargo vdev deprecation sync-cue`. DO NOT EDIT.\n\
         package metadata\n\
         \n\
         deprecations_pending: [\n\
         {pending_block}\
         ]\n\
         \n\
         deprecations_enacted: [\n\
         {enacted_block}\
         ]\n"
    )
}

fn render_pending_block(entries: &[DeprecationEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let mut s = String::from("\t{\n");
            s.push_str(&format!("\t\twhat:             {}\n", json!(e.what)));
            s.push_str(&format!(
                "\t\tdeprecated_since: {}\n",
                json!(e.deprecated_since.to_string())
            ));
            if !e.description.is_empty() {
                s.push_str("\t\tdescription: #\"\"\"\n");
                for line in e.description.lines() {
                    s.push_str(&format!("\t\t\t{line}\n"));
                }
                s.push_str("\t\t\t\"\"\"#\n");
            }
            s.push_str("\t},\n");
            s
        })
        .collect()
}

fn render_enacted_block(entries: &[EnactedEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let mut s = String::from("\t{\n");
            s.push_str(&format!("\t\twhat:             {}\n", json!(e.what)));
            s.push_str(&format!("\t\tdeprecated_since: {}\n", json!(e.deprecated_since)));
            s.push_str(&format!("\t\tremoved_in:       {}\n", json!(e.removed_in)));
            if !e.description.is_empty() {
                s.push_str("\t\tdescription: #\"\"\"\n");
                for line in e.description.lines() {
                    s.push_str(&format!("\t\t\t{line}\n"));
                }
                s.push_str("\t\t\t\"\"\"#\n");
            }
            s.push_str("\t},\n");
            s
        })
        .collect()
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
    }
}
