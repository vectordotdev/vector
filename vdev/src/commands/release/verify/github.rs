use std::{collections::HashMap, env};

use anyhow::{Context as _, Result, bail};

use super::{resolve_version, util};

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
        let summary = verify(&version)?;
        println!("OK: {summary}");
        Ok(())
    }
}

pub fn verify(version: &str) -> Result<String> {
    let client = util::stream_client()?;

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

    let sums_name = format!("vector-{version}-SHA256SUMS");
    let sums_url = assets
        .get(&sums_name)
        .ok_or_else(|| anyhow::anyhow!("{sums_name} missing from assets"))?;
    let sums = util::parse_sha256sums(&util::fetch_text(&client, sums_url)?);

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
    let actual = util::stream_sha256(client, url)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(actual)
    } else {
        bail!("checksum mismatch: got {actual}, expected {expected}");
    }
}

#[cfg(test)]
mod tests {
    use super::expected_assets;

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
}
