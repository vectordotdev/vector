use anyhow::{Context as _, Result, bail};

use super::{resolve_version, util};

const DEFAULT_BASE_URL: &str = "https://yum.vector.dev";
const DEFAULT_SUITE: &str = "stable";
const DEFAULT_COMPONENT: &str = "vector-0";

// Architectures the Datadog YUM repo actually serves a Vector rpm for. Derived from the
// `build-linux-packages` matrix in `.github/workflows/publish.yml`: every `-linux-gnu` /
// `-linux-gnueabihf` target that builds an rpm maps to one of these YUM arch names.
//   x86_64-unknown-linux-gnu        -> x86_64
//   aarch64-unknown-linux-gnu       -> aarch64
//   armv7-unknown-linux-gnueabihf   -> armv7hl
const ARCHES: &[&str] = &["x86_64", "aarch64", "armv7hl"];

/// Verify the Datadog YUM repo serves a Vector release across every expected architecture.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Version to verify (e.g. `0.55.0`). Defaults to the most recent `v*` git tag.
    version: Option<String>,

    /// Base URL of the YUM repo.
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    url: String,

    /// YUM suite name.
    #[arg(long, default_value = DEFAULT_SUITE)]
    suite: String,

    /// YUM component name.
    #[arg(long, default_value = DEFAULT_COMPONENT)]
    component: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = resolve_version(self.version)?;
        let summary = verify_with(&version, &self.url, &self.suite, &self.component)?;
        println!("OK: {summary}");
        Ok(())
    }
}

pub fn verify(version: &str) -> Result<String> {
    verify_with(version, DEFAULT_BASE_URL, DEFAULT_SUITE, DEFAULT_COMPONENT)
}

fn verify_with(version: &str, base_url: &str, suite: &str, component: &str) -> Result<String> {
    let client = util::client()?;

    info!("Checking Vector {version} in {base_url}/{suite}/{component}");

    let mut failures = 0usize;
    for arch in ARCHES {
        match check_arch(&client, base_url, suite, component, arch, version) {
            Ok((filename, rpm_size)) => {
                println!("  {arch:<8} OK    {filename} ({rpm_size} bytes)");
            }
            Err(e) => {
                failures += 1;
                println!("  {arch:<8} FAIL  {e:#}");
            }
        }
    }

    if failures > 0 {
        bail!(
            "{failures}/{} architectures missing {version}",
            ARCHES.len()
        );
    }
    Ok(format!("{}/{} arches OK", ARCHES.len(), ARCHES.len()))
}

fn check_arch(
    client: &reqwest::blocking::Client,
    base_url: &str,
    suite: &str,
    component: &str,
    arch: &str,
    version: &str,
) -> Result<(String, u64)> {
    let arch_base = format!("{base_url}/{suite}/{component}/{arch}");

    let repomd_url = format!("{arch_base}/repodata/repomd.xml");
    let repomd = util::fetch_text(client, &repomd_url)?;
    let primary_href = find_primary_href(&repomd)
        .with_context(|| format!("no primary data entry in {repomd_url}"))?;

    let primary_url = format!("{arch_base}/{primary_href}");
    let primary = util::fetch_gz_text(client, &primary_url)?;
    let filename = find_rpm_location(&primary, version)
        .with_context(|| format!("version {version}-1 not found in {primary_url}"))?;

    let rpm_url = format!("{arch_base}/{filename}");
    let size = util::head_size(client, &rpm_url)?;
    Ok((filename, size))
}

// Pull the `<location href="..."/>` out of the `<data type="primary">` block of a repomd.xml.
// repomd.xml is small and well-formed; scanning for literal tags is plenty here.
fn find_primary_href(repomd: &str) -> Option<String> {
    let start = repomd.find(r#"<data type="primary">"#)?;
    let tail = &repomd[start..];
    let end = tail.find("</data>")?;
    extract_href(&tail[..end])
}

// Scan primary.xml for the `<package type="rpm">` stanza matching `ver="{version}" rel="1"`
// and return its `<location href="..."/>` (the relative rpm path inside the arch dir).
fn find_rpm_location(primary: &str, version: &str) -> Option<String> {
    let wanted_version = format!(r#"ver="{version}""#);
    // Split on `<package type=` rather than `<package` — the latter also matches `<packager>`
    // tags, which appear inside every package stanza and would split it mid-body.
    for stanza in primary.split("<package type=") {
        // Each `<package>...</package>` contains a single `<version epoch="0" ver="X" rel="Y"/>`
        // line. We look for both `ver="X"` and `rel="1"` on the same `<version>` line.
        if !stanza.contains(&wanted_version) {
            continue;
        }
        let version_line = stanza
            .lines()
            .find(|line| line.contains("<version ") && line.contains(&wanted_version))?;
        if !version_line.contains(r#" rel="1""#) {
            continue;
        }
        return extract_href(stanza);
    }
    None
}

// Extract the first `href="..."` attribute from a `<location ... />` element within `block`.
fn extract_href(block: &str) -> Option<String> {
    let loc_start = block.find("<location ")?;
    let rest = &block[loc_start..];
    let tag_end = rest.find("/>")?;
    let tag = &rest[..tag_end];
    let href_start = tag.find(r#"href=""#)? + r#"href=""#.len();
    let value = &tag[href_start..];
    let href_end = value.find('"')?;
    Some(value[..href_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{find_primary_href, find_rpm_location};

    #[test]
    fn extracts_primary_href_from_repomd() {
        let repomd = r#"<?xml version="1.0"?>
<repomd>
<data type="filelists">
  <location href="repodata/abc-filelists.xml.gz"/>
</data>
<data type="primary">
  <checksum type="sha256">deadbeef</checksum>
  <location href="repodata/abc-primary.xml.gz"/>
</data>
</repomd>"#;
        assert_eq!(
            find_primary_href(repomd).as_deref(),
            Some("repodata/abc-primary.xml.gz"),
        );
    }

    #[test]
    fn returns_none_when_no_primary_block() {
        let repomd = r#"<repomd><data type="other"><location href="x.xml.gz"/></data></repomd>"#;
        assert_eq!(find_primary_href(repomd), None);
    }

    #[test]
    fn finds_matching_package_stanza() {
        let primary = r#"<?xml version="1.0"?>
<metadata>
<package type="rpm">
  <name>vector</name>
  <arch>x86_64</arch>
  <version epoch="0" ver="0.54.0" rel="1"/>
<location href="vector-0.54.0-1.x86_64.rpm"/>
</package>
<package type="rpm">
  <name>vector</name>
  <arch>x86_64</arch>
  <version epoch="0" ver="0.55.0" rel="1"/>
<location href="vector-0.55.0-1.x86_64.rpm"/>
</package>
</metadata>"#;
        assert_eq!(
            find_rpm_location(primary, "0.55.0").as_deref(),
            Some("vector-0.55.0-1.x86_64.rpm"),
        );
        assert_eq!(
            find_rpm_location(primary, "0.54.0").as_deref(),
            Some("vector-0.54.0-1.x86_64.rpm"),
        );
        assert_eq!(find_rpm_location(primary, "0.56.0"), None);
    }

    // Real `primary.xml` stanzas include a `<packager></packager>` tag, which contains the
    // literal prefix `<package`. A naive split on `<package` would chop the stanza in half
    // mid-body, losing the `<location>` tag that comes after it.
    #[test]
    fn packager_tag_does_not_break_split() {
        let primary = r#"<metadata>
<package type="rpm">
  <name>vector</name>
  <version epoch="0" ver="0.55.0" rel="1"/>
  <packager></packager>
<location href="vector-0.55.0-1.x86_64.rpm"/>
</package>
</metadata>"#;
        assert_eq!(
            find_rpm_location(primary, "0.55.0").as_deref(),
            Some("vector-0.55.0-1.x86_64.rpm"),
        );
    }

    #[test]
    fn ignores_non_rel_1_builds() {
        let primary = r#"<package type="rpm">
  <version epoch="0" ver="0.55.0" rel="2"/>
<location href="vector-0.55.0-2.x86_64.rpm"/>
</package>"#;
        assert_eq!(find_rpm_location(primary, "0.55.0"), None);
    }

    #[test]
    fn empty_index_has_no_match() {
        assert_eq!(find_rpm_location("", "0.55.0"), None);
    }
}
