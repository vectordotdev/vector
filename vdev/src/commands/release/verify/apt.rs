use std::{io::Read, time::Duration};

use anyhow::{Context as _, Result, bail};
use flate2::read::GzDecoder;

use super::{VerifyOutcome, resolve_version};

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
        match verify_inner(&version, &self.url, &self.suite, &self.component) {
            Ok(summary) => {
                println!("OK: {summary}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

pub fn verify(version: &str) -> VerifyOutcome {
    match verify_inner(version, DEFAULT_BASE_URL, DEFAULT_SUITE, DEFAULT_COMPONENT) {
        Ok(summary) => VerifyOutcome::Ok(summary),
        Err(e) => VerifyOutcome::Failed(e),
    }
}

fn verify_inner(version: &str, base_url: &str, suite: &str, component: &str) -> Result<String> {
    let debian_version = format!("{version}-1");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    info!("Checking Vector {version} in {base_url}/dists/{suite}/{component}");

    let mut failures = 0usize;
    for arch in ARCHES {
        match check_arch(&client, base_url, suite, component, arch, &debian_version) {
            Ok(ArchResult { filename, deb_size }) => {
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

struct ArchResult {
    filename: String,
    deb_size: u64,
}

fn check_arch(
    client: &reqwest::blocking::Client,
    base_url: &str,
    suite: &str,
    component: &str,
    arch: &str,
    debian_version: &str,
) -> Result<ArchResult> {
    let index_url = format!("{base_url}/dists/{suite}/{component}/binary-{arch}/Packages.gz");
    let body = client
        .get(&index_url)
        .send()
        .with_context(|| format!("fetching {index_url}"))?
        .error_for_status()
        .with_context(|| format!("fetching {index_url}"))?
        .bytes()
        .with_context(|| format!("reading {index_url}"))?;

    let mut decoded = String::new();
    GzDecoder::new(body.as_ref())
        .read_to_string(&mut decoded)
        .with_context(|| format!("gunzipping {index_url}"))?;

    let filename = find_filename(&decoded, debian_version)
        .with_context(|| format!("version {debian_version} not found in {index_url}"))?;

    let deb_url = format!("{base_url}/{filename}");
    let head = client
        .head(&deb_url)
        .send()
        .with_context(|| format!("HEAD {deb_url}"))?
        .error_for_status()
        .with_context(|| format!("HEAD {deb_url}"))?;
    let deb_size = head
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    Ok(ArchResult { filename, deb_size })
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
