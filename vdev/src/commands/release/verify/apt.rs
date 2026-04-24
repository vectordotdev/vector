use anyhow::{Context as _, Result, bail};

use super::{resolve_version, util};

const DEFAULT_BASE_URL: &str = "https://apt.vector.dev";
const DEFAULT_COMPONENT: &str = "vector-0";
const DEFAULT_SUITE: &str = "stable";

// Architectures the Datadog APT repo actually serves a Vector deb for. `binary-all` and
// `binary-i386` exist in the repo layout but have always been empty for Vector, so we skip them.
// `x86_64` is a legacy Datadog alias that points at the same amd64 deb.
const ARCHES: &[&str] = &["amd64", "arm64", "armhf", "x86_64"];

/// Verify the Datadog APT repo serves a Vector release across every expected architecture.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Version to verify (e.g. `0.55.0`). Defaults to the most recent `v*` git tag.
    version: Option<String>,

    /// Base URL of the APT repo.
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    url: String,

    /// APT suite name.
    #[arg(long, default_value = DEFAULT_SUITE)]
    suite: String,

    /// APT component name.
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
    let debian_version = format!("{version}-1");
    let client = util::client()?;

    info!("Checking Vector {version} in {base_url}/dists/{suite}/{component}");

    let mut failures = 0usize;
    for arch in ARCHES {
        match check_arch(&client, base_url, suite, component, arch, &debian_version) {
            Ok((filename, deb_size)) => {
                println!("  {arch:<8} OK    {filename} ({deb_size} bytes)");
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
    debian_version: &str,
) -> Result<(String, u64)> {
    let index_url = format!("{base_url}/dists/{suite}/{component}/binary-{arch}/Packages.gz");
    let packages = util::fetch_gz_text(client, &index_url)?;
    let filename = find_filename(&packages, debian_version)
        .with_context(|| format!("version {debian_version} not found in {index_url}"))?;
    let deb_url = format!("{base_url}/{filename}");
    let size = util::head_size(client, &deb_url)?;
    Ok((filename, size))
}

// Walk the Packages stanzas (separated by blank lines) and return the `Filename:` of
// the one whose `Version:` matches.
fn find_filename(packages: &str, debian_version: &str) -> Option<String> {
    for stanza in packages.split("\n\n") {
        let mut version = None;
        let mut filename = None;
        for line in stanza.lines() {
            if let Some(rest) = line.strip_prefix("Version:") {
                version = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("Filename:") {
                filename = Some(rest.trim().to_string());
            }
        }
        if version.as_deref() == Some(debian_version) {
            return filename;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::find_filename;

    #[test]
    fn finds_matching_stanza() {
        let packages = "\
Package: vector
Version: 0.54.0-1
Filename: pool/v/ve/vector_0.54.0-1_amd64.deb

Package: vector
Version: 0.55.0-1
Filename: pool/v/ve/vector_0.55.0-1_amd64.deb
";
        assert_eq!(
            find_filename(packages, "0.55.0-1").as_deref(),
            Some("pool/v/ve/vector_0.55.0-1_amd64.deb"),
        );
        assert_eq!(
            find_filename(packages, "0.54.0-1").as_deref(),
            Some("pool/v/ve/vector_0.54.0-1_amd64.deb"),
        );
        assert_eq!(find_filename(packages, "0.56.0-1"), None);
    }

    #[test]
    fn empty_index_has_no_match() {
        assert_eq!(find_filename("", "0.55.0-1"), None);
    }
}
