use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use super::{VerifyOutcome, resolve_version};

const DEFAULT_BASE_URL: &str = "https://packages.timber.io/vector";

// Expected artifacts for every versioned release of Vector, parameterised by `{version}`.
// Derived from the `build-linux-packages` / `build-apple-darwin-packages` /
// `build-x86_64-pc-windows-msvc-packages` / `generate-sha256sum` jobs in
// `.github/workflows/publish.yml` and the matching `make package-*` targets. Keep this list
// in sync with the `github` probe's expected asset list.
const ARTIFACT_TEMPLATES: &[&str] = &[
    // Linux tarballs
    "vector-{v}-x86_64-unknown-linux-gnu.tar.gz",
    "vector-{v}-x86_64-unknown-linux-musl.tar.gz",
    "vector-{v}-aarch64-unknown-linux-gnu.tar.gz",
    "vector-{v}-aarch64-unknown-linux-musl.tar.gz",
    "vector-{v}-armv7-unknown-linux-gnueabihf.tar.gz",
    "vector-{v}-armv7-unknown-linux-musleabihf.tar.gz",
    "vector-{v}-arm-unknown-linux-gnueabi.tar.gz",
    "vector-{v}-arm-unknown-linux-musleabi.tar.gz",
    // Debian packages (only produced for targets whose `package-%-all` target includes a .deb)
    "vector_{v}-1_amd64.deb",
    "vector_{v}-1_arm64.deb",
    "vector_{v}-1_armhf.deb",
    "vector_{v}-1_armel.deb",
    // RPM packages
    "vector-{v}-1.x86_64.rpm",
    "vector-{v}-1.aarch64.rpm",
    "vector-{v}-1.armv7hl.rpm",
    // Windows
    "vector-{v}-x86_64-pc-windows-msvc.zip",
    "vector-{v}-x64.msi",
    // macOS
    "vector-{v}-arm64-apple-darwin.tar.gz",
    // Checksum manifest
    "vector-{v}-SHA256SUMS",
];

// Representative artifacts checksummed end-to-end. One deb (the most-downloaded package
// format) and one Linux tarball (the canonical `install.sh` target) give decent coverage
// without paying for ~30 MB per sample x18.
const CHECKSUM_SAMPLES: &[&str] = &[
    "vector_{v}-1_amd64.deb",
    "vector-{v}-x86_64-unknown-linux-gnu.tar.gz",
];

// Representative file used for aliased-path probes. Picked because the release-s3.sh
// verification step uses this exact file, so we mirror its contract.
const ALIAS_PROBE_TEMPLATE: &str = "vector-{v}-x86_64-unknown-linux-gnu.tar.gz";
const ALIAS_PROBE_LATEST_TEMPLATE: &str = "vector-latest-x86_64-unknown-linux-gnu.tar.gz";

/// Verify `packages.timber.io/vector/` has every release artifact at the exact-version
/// path AND at the aliased paths (`{major}.X`, `{major}.{minor}.X`, `latest`).
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Version to verify (e.g. `0.55.0`). Defaults to the most recent `v*` git tag.
    version: Option<String>,

    /// Base URL of the artifact bucket (no trailing slash).
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    url: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = resolve_version(self.version)?;
        match verify_inner(&version, &self.url) {
            Ok(summary) => {
                println!("OK: {summary}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

pub fn verify(version: &str) -> VerifyOutcome {
    match verify_inner(version, DEFAULT_BASE_URL) {
        Ok(summary) => VerifyOutcome::Ok(summary),
        Err(e) => VerifyOutcome::Failed(e),
    }
}

fn verify_inner(version: &str, base_url: &str) -> Result<String> {
    info!("Checking Vector {version} at {base_url}/{version}/");

    // Short timeout for HEADs; the sha256 GETs stream large bodies so they get their own
    // client with a generous timeout.
    let head_client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let stream_client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let expected: Vec<String> = ARTIFACT_TEMPLATES
        .iter()
        .map(|t| t.replace("{v}", version))
        .collect();

    let missing = check_exact_path(&head_client, base_url, version, &expected);
    let alias_failures = check_aliases(&head_client, base_url, version)?;
    let (checksum_ok, checksum_fail) = check_checksums(&stream_client, base_url, version)?;

    if missing > 0 || alias_failures > 0 || checksum_fail > 0 {
        bail!(
            "{missing}/{} files missing at exact path, {alias_failures} alias failure(s), {checksum_fail}/{} checksum failure(s)",
            expected.len(),
            CHECKSUM_SAMPLES.len(),
        );
    }

    Ok(format!(
        "{}/{} files present + {checksum_ok}/{} checksums verified",
        expected.len(),
        expected.len(),
        CHECKSUM_SAMPLES.len(),
    ))
}

// HEAD every expected file at `/<version>/`. Returns the number missing/unreachable.
fn check_exact_path(
    client: &reqwest::blocking::Client,
    base_url: &str,
    version: &str,
    expected: &[String],
) -> usize {
    let mut missing = 0usize;
    for filename in expected {
        let url = format!("{base_url}/{version}/{filename}");
        match head_size(client, &url) {
            Ok(size) => println!("  exact    OK    {filename} ({size} bytes)"),
            Err(e) => {
                missing += 1;
                println!("  exact    FAIL  {filename}: {e:#}");
            }
        }
    }
    missing
}

// Probe the three aliased paths (`<major>.<minor>.X`, `<major>.X`, `latest`). Returns
// the number of failed probes across both the versioned-filename and the alias-named
// redirect form.
fn check_aliases(
    client: &reqwest::blocking::Client,
    base_url: &str,
    version: &str,
) -> Result<usize> {
    let (major, minor) = parse_major_minor(version)?;
    let aliases = [
        format!("{major}.{minor}.X"),
        format!("{major}.X"),
        "latest".to_string(),
    ];
    let versioned_alias_file = ALIAS_PROBE_TEMPLATE.replace("{v}", version);
    let latest_named_file = ALIAS_PROBE_LATEST_TEMPLATE;

    let mut failures = 0usize;
    for alias in &aliases {
        // The versioned filename should be served directly (200) at every alias path.
        let url = format!("{base_url}/{alias}/{versioned_alias_file}");
        match head_size(client, &url) {
            Ok(size) => println!("  {alias:<8} OK    {versioned_alias_file} ({size} bytes)"),
            Err(e) => {
                failures += 1;
                println!("  {alias:<8} FAIL  {versioned_alias_file}: {e:#}");
            }
        }
    }

    // The `latest`-named alias (e.g. `vector-latest-...`) is a website-redirect -> 200.
    let url = format!("{base_url}/latest/{latest_named_file}");
    match head_follow(client, &url) {
        Ok(size) => println!("  latest   OK    {latest_named_file} -> 200 ({size} bytes)"),
        Err(e) => {
            failures += 1;
            println!("  latest   FAIL  {latest_named_file}: {e:#}");
        }
    }

    Ok(failures)
}

// Fetch SHA256SUMS and stream-hash the CHECKSUM_SAMPLES files against it. Returns
// (ok_count, fail_count). Failures include: sample missing from SHA256SUMS, download
// failure, and digest mismatch.
fn check_checksums(
    client: &reqwest::blocking::Client,
    base_url: &str,
    version: &str,
) -> Result<(usize, usize)> {
    let sha_url = format!("{base_url}/{version}/vector-{version}-SHA256SUMS");
    let sha_body = client
        .get(&sha_url)
        .send()
        .with_context(|| format!("fetching {sha_url}"))?
        .error_for_status()
        .with_context(|| format!("fetching {sha_url}"))?
        .text()
        .with_context(|| format!("reading {sha_url}"))?;
    let sums = parse_sha256sums(&sha_body);

    let mut ok = 0usize;
    let mut fail = 0usize;
    for template in CHECKSUM_SAMPLES {
        let filename = template.replace("{v}", version);
        let Some((_, expected_hex)) = sums.iter().find(|(name, _)| name == &filename) else {
            fail += 1;
            println!("  checksum FAIL  {filename}: not listed in SHA256SUMS");
            continue;
        };
        let url = format!("{base_url}/{version}/{filename}");
        match stream_sha256(client, &url) {
            Ok(actual) if &actual == expected_hex => {
                ok += 1;
                println!("  checksum OK    {filename} sha256={expected_hex}");
            }
            Ok(actual) => {
                fail += 1;
                println!("  checksum FAIL  {filename}: expected {expected_hex}, got {actual}");
            }
            Err(e) => {
                fail += 1;
                println!("  checksum FAIL  {filename}: {e:#}");
            }
        }
    }
    Ok((ok, fail))
}

fn head_size(client: &reqwest::blocking::Client, url: &str) -> Result<u64> {
    let resp = client
        .head(url)
        .send()
        .with_context(|| format!("HEAD {url}"))?
        .error_for_status()
        .with_context(|| format!("HEAD {url}"))?;
    Ok(content_length(&resp).unwrap_or(0))
}

// HEAD with redirects followed; expects a final 200. Used for the `latest`-named alias
// that points at a website-redirect to the real file.
fn head_follow(client: &reqwest::blocking::Client, url: &str) -> Result<u64> {
    // reqwest's blocking client follows redirects on HEAD by default (up to 10 hops).
    let resp = client
        .head(url)
        .send()
        .with_context(|| format!("HEAD {url}"))?
        .error_for_status()
        .with_context(|| format!("HEAD {url}"))?;
    Ok(content_length(&resp).unwrap_or(0))
}

fn content_length(resp: &reqwest::blocking::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
}

// Stream-download `url` through SHA-256 without buffering the whole body. Returns the
// lowercase hex digest. Uses an 8 KiB copy buffer via `io::copy`.
fn stream_sha256(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    use std::io::Write;

    struct Hasher(Sha256);
    impl Write for Hasher {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.update(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;

    let mut hasher = Hasher(Sha256::new());
    std::io::copy(&mut resp, &mut hasher).with_context(|| format!("streaming {url}"))?;
    Ok(hex::encode(hasher.0.finalize()))
}

// Parse GNU `sha256sum`-style output: each line is `<hex>  <filename>` (two spaces between).
// Tolerates `<hex> *<filename>` (binary-mode marker) and trims blank lines.
fn parse_sha256sums(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, "  ");
        let (Some(hex), Some(rest)) = (parts.next(), parts.next()) else {
            continue;
        };
        let filename = rest.strip_prefix('*').unwrap_or(rest).to_string();
        out.push((filename, hex.to_string()));
    }
    out
}

// Split "0.55.0" -> ("0", "55"). We don't care about the patch digit here; the aliases
// `{major}.X` and `{major}.{minor}.X` only use the first two components.
fn parse_major_minor(version: &str) -> Result<(&str, &str)> {
    let mut parts = version.splitn(3, '.');
    let major = parts
        .next()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("version {version:?} has no major component"))?;
    let minor = parts
        .next()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("version {version:?} has no minor component"))?;
    Ok((major, minor))
}

#[cfg(test)]
mod tests {
    use super::{parse_major_minor, parse_sha256sums};

    #[test]
    fn parses_sha256sums_two_space_delimited() {
        let body = "\
abc123  vector-0.55.0-x86_64-unknown-linux-gnu.tar.gz
deadbeef  vector_0.55.0-1_amd64.deb

";
        let parsed = parse_sha256sums(body);
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0],
            (
                "vector-0.55.0-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                "abc123".to_string()
            )
        );
        assert_eq!(
            parsed[1],
            (
                "vector_0.55.0-1_amd64.deb".to_string(),
                "deadbeef".to_string()
            )
        );
    }

    #[test]
    fn parses_sha256sums_binary_marker() {
        let body = "abc123  *vector-0.55.0-x86_64-unknown-linux-gnu.tar.gz\n";
        let parsed = parse_sha256sums(body);
        assert_eq!(parsed[0].0, "vector-0.55.0-x86_64-unknown-linux-gnu.tar.gz");
    }

    #[test]
    fn parses_major_minor() {
        assert_eq!(parse_major_minor("0.55.0").unwrap(), ("0", "55"));
        assert_eq!(parse_major_minor("1.2.3-pre").unwrap(), ("1", "2"));
        assert!(parse_major_minor("1").is_err());
        assert!(parse_major_minor("").is_err());
    }
}
