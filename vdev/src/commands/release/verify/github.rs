use std::{collections::HashMap, env, time::Duration};

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use super::{VerifyOutcome, resolve_version};

const USER_AGENT: &str = "vdev-release-verify";
const API_BASE: &str = "https://api.github.com";
const REPO: &str = "vectordotdev/vector";

// Linux/macOS targets that publish `vector-<ver>-<target>.tar.gz` per publish.yml's
// build-linux / build-apple-darwin matrices.
const TARBALL_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "armv7-unknown-linux-gnueabihf",
    "armv7-unknown-linux-musleabihf",
    "arm-unknown-linux-gnueabi",
    "arm-unknown-linux-musleabi",
    "arm64-apple-darwin",
];

const DEB_ARCHES: &[&str] = &["amd64", "arm64", "armhf", "armel"];
const RPM_ARCHES: &[&str] = &["x86_64", "aarch64", "armv7hl"];

/// Verify GitHub release assets and SHA256SUMS for a Vector release.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Version to verify (e.g. `0.55.0`). Defaults to the most recent `v*` git tag.
    version: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = resolve_version(self.version)?;
        match verify_inner(&version) {
            Ok(summary) => {
                println!("OK: {summary}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

pub fn verify(version: &str) -> VerifyOutcome {
    match verify_inner(version) {
        Ok(summary) => VerifyOutcome::Ok(summary),
        Err(e) => VerifyOutcome::Failed(e),
    }
}

fn verify_inner(version: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent(USER_AGENT)
        .build()?;

    info!("Fetching GitHub release v{version} from {REPO}");

    let release = fetch_release(&client, version)?;
    let assets =
        parse_assets(&release).with_context(|| format!("parsing assets for release v{version}"))?;

    let expected = expected_assets(version);

    let mut missing = Vec::new();
    for name in &expected {
        if assets.contains_key(name) {
            println!("  OK    {name}");
        } else {
            println!("  MISS  {name}");
            missing.push(name.clone());
        }
    }

    if !missing.is_empty() {
        bail!(
            "{}/{} expected assets missing: {}",
            missing.len(),
            expected.len(),
            missing.join(", "),
        );
    }

    // SHA256SUMS is already in `expected`/`assets`, but grab its URL for download.
    let sums_name = format!("vector-{version}-SHA256SUMS");
    let sums_url = assets
        .get(&sums_name)
        .ok_or_else(|| anyhow::anyhow!("{sums_name} missing from assets"))?;
    let sums_body = client
        .get(sums_url)
        .send()
        .with_context(|| format!("fetching {sums_name}"))?
        .error_for_status()
        .with_context(|| format!("fetching {sums_name}"))?
        .text()
        .with_context(|| format!("reading {sums_name}"))?;
    let sums = parse_sha256sums(&sums_body);

    let samples = [
        format!("vector_{version}-1_amd64.deb"),
        format!("vector-{version}-x86_64-unknown-linux-gnu.tar.gz"),
        format!("vector-{version}-arm64-apple-darwin.tar.gz"),
    ];

    let mut sum_failures = 0usize;
    for sample in &samples {
        match verify_sample(&client, sample, &assets, &sums) {
            Ok(digest) => println!("  SHA   {sample} {digest}"),
            Err(e) => {
                sum_failures += 1;
                println!("  SHA   {sample} FAIL {e:#}");
            }
        }
    }

    if sum_failures > 0 {
        bail!("{sum_failures}/{} sampled checksums failed", samples.len());
    }

    Ok(format!(
        "{} assets present, {}/{} sampled checksums match",
        expected.len(),
        samples.len(),
        samples.len(),
    ))
}

fn expected_assets(version: &str) -> Vec<String> {
    let mut out = Vec::new();
    for target in TARBALL_TARGETS {
        out.push(format!("vector-{version}-{target}.tar.gz"));
    }
    for arch in DEB_ARCHES {
        out.push(format!("vector_{version}-1_{arch}.deb"));
    }
    for arch in RPM_ARCHES {
        out.push(format!("vector-{version}-1.{arch}.rpm"));
    }
    out.push(format!("vector-{version}-x86_64-pc-windows-msvc.zip"));
    out.push(format!("vector-{version}-x64.msi"));
    out.push(format!("vector-{version}-SHA256SUMS"));
    out
}

fn fetch_release(client: &reqwest::blocking::Client, version: &str) -> Result<serde_json::Value> {
    let url = format!("{API_BASE}/repos/{REPO}/releases/tags/v{version}");
    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = req.send().with_context(|| format!("fetching {url}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        // Fall back to `gh api`, which uses the user's auth.
        return fetch_release_via_gh(version)
            .with_context(|| format!("GitHub API returned {status} for {url}; gh fallback"));
    }
    let body = resp
        .error_for_status()
        .with_context(|| format!("GitHub API status for {url}"))?
        .text()
        .with_context(|| format!("reading body of {url}"))?;
    serde_json::from_str(&body).with_context(|| format!("parsing JSON from {url}"))
}

fn fetch_release_via_gh(version: &str) -> Result<serde_json::Value> {
    let path = format!("repos/{REPO}/releases/tags/v{version}");
    let output = std::process::Command::new("gh")
        .args(["api", &path])
        .output()
        .context("invoking `gh api` (is the GitHub CLI installed and authenticated?)")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`gh api {path}` failed: {stderr}");
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("parsing JSON from `gh api {path}`"))
}

// Returns a map of asset name -> browser_download_url.
fn parse_assets(release: &serde_json::Value) -> Result<HashMap<String, String>> {
    let assets = release
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .context("release JSON has no `assets` array")?;
    let mut out = HashMap::with_capacity(assets.len());
    for asset in assets {
        let name = asset
            .get("name")
            .and_then(serde_json::Value::as_str)
            .context("asset missing `name`")?;
        let url = asset
            .get("browser_download_url")
            .and_then(serde_json::Value::as_str)
            .context("asset missing `browser_download_url`")?;
        out.insert(name.to_string(), url.to_string());
    }
    Ok(out)
}

// Lines look like: `<hex>  <filename>` (two spaces, coreutils style).
fn parse_sha256sums(body: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((digest, name)) = line.split_once("  ") else {
            continue;
        };
        let name = name.trim_start_matches('*').trim();
        out.insert(name.to_string(), digest.trim().to_string());
    }
    out
}

fn verify_sample(
    client: &reqwest::blocking::Client,
    name: &str,
    assets: &HashMap<String, String>,
    sums: &HashMap<String, String>,
) -> Result<String> {
    let url = assets
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("asset {name} not in release"))?;
    let expected = sums
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("{name} not in SHA256SUMS"))?;

    let mut resp = client
        .get(url)
        .send()
        .with_context(|| format!("downloading {name}"))?
        .error_for_status()
        .with_context(|| format!("downloading {name}"))?;

    let mut hasher = Sha256Writer(Sha256::new());
    resp.copy_to(&mut hasher)
        .with_context(|| format!("streaming {name} into hasher"))?;
    let actual = hex::encode(hasher.0.finalize());

    if actual.eq_ignore_ascii_case(expected) {
        Ok(actual)
    } else {
        bail!("checksum mismatch: got {actual}, expected {expected}");
    }
}

// Thin `io::Write` adapter so `Response::copy_to` can stream into a Sha256 hasher
// without buffering the asset in memory.
struct Sha256Writer(Sha256);

impl std::io::Write for Sha256Writer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{expected_assets, parse_sha256sums};

    #[test]
    fn expected_asset_list_is_complete() {
        let names = expected_assets("0.55.0");
        // 9 tarballs + 4 debs + 3 rpms + zip + msi + SHA256SUMS = 19.
        assert_eq!(names.len(), 19);
        assert!(names.contains(&"vector-0.55.0-x86_64-unknown-linux-gnu.tar.gz".to_string()));
        assert!(names.contains(&"vector_0.55.0-1_amd64.deb".to_string()));
        assert!(names.contains(&"vector-0.55.0-1.x86_64.rpm".to_string()));
        assert!(names.contains(&"vector-0.55.0-x86_64-pc-windows-msvc.zip".to_string()));
        assert!(names.contains(&"vector-0.55.0-x64.msi".to_string()));
        assert!(names.contains(&"vector-0.55.0-SHA256SUMS".to_string()));
    }

    #[test]
    fn parses_coreutils_sha256sums() {
        let body = "\
abc123  vector-0.55.0-x64.msi
deadbeef  vector_0.55.0-1_amd64.deb
# comment line
";
        let sums = parse_sha256sums(body);
        assert_eq!(
            sums.get("vector-0.55.0-x64.msi").map(String::as_str),
            Some("abc123")
        );
        assert_eq!(
            sums.get("vector_0.55.0-1_amd64.deb").map(String::as_str),
            Some("deadbeef"),
        );
        assert_eq!(sums.len(), 2);
    }

    #[test]
    fn parses_binary_mode_sha256sums() {
        // coreutils writes `*filename` for binary mode; strip the leading `*`.
        let body = "feedface  *vector-0.55.0-x64.msi\n";
        let sums = parse_sha256sums(body);
        assert_eq!(
            sums.get("vector-0.55.0-x64.msi").map(String::as_str),
            Some("feedface")
        );
    }
}
