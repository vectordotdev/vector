use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use super::{VerifyOutcome, resolve_version};

const DEFAULT_FORMULA_URL: &str =
    "https://raw.githubusercontent.com/vectordotdev/homebrew-brew/master/Formula/vector.rb";

/// Verify the Homebrew formula (`vectordotdev/homebrew-brew`) references the release.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Version to verify (e.g. `0.55.0`). Defaults to the most recent `v*` git tag.
    version: Option<String>,

    /// Raw URL of the `vector.rb` Homebrew formula.
    #[arg(long, default_value = DEFAULT_FORMULA_URL)]
    formula_url: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = resolve_version(self.version)?;
        match verify_inner(&version, &self.formula_url) {
            Ok(summary) => {
                println!("OK: {summary}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

pub fn verify(version: &str) -> VerifyOutcome {
    match verify_inner(version, DEFAULT_FORMULA_URL) {
        Ok(summary) => VerifyOutcome::Ok(summary),
        Err(e) => VerifyOutcome::Failed(e),
    }
}

fn verify_inner(version: &str, formula_url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    info!("Fetching Homebrew formula from {formula_url}");
    let formula = client
        .get(formula_url)
        .send()
        .with_context(|| format!("fetching {formula_url}"))?
        .error_for_status()
        .with_context(|| format!("fetching {formula_url}"))?
        .text()
        .with_context(|| format!("reading {formula_url}"))?;

    let formula_version =
        extract_version(&formula).context("no `version \"...\"` line found in formula")?;
    if formula_version != version {
        bail!("formula version mismatch: formula says {formula_version:?}, expected {version:?}");
    }
    println!("  version {formula_version} OK");

    let pairs = extract_url_sha_pairs(&formula);
    if pairs.is_empty() {
        bail!("no url+sha256 pairs found in formula");
    }

    let version_segment = format!("/vector/{version}/");
    let mut verified = 0usize;
    let mut failures = 0usize;
    let mut skipped = 0usize;
    for (url, sha) in &pairs {
        if !url.contains(&version_segment) {
            println!("  SKIP  {url} (not for {version})");
            skipped += 1;
            continue;
        }
        if !is_hex64(sha) {
            failures += 1;
            println!("  FAIL  {url}: sha256 {sha:?} is not a 64-char hex digest");
            continue;
        }
        match download_and_digest(&client, url) {
            Ok(digest) => {
                if digest.eq_ignore_ascii_case(sha) {
                    verified += 1;
                    println!("  OK    {url} sha256={digest}");
                } else {
                    failures += 1;
                    println!("  FAIL  {url}: digest mismatch (formula={sha}, computed={digest})");
                }
            }
            Err(e) => {
                failures += 1;
                println!("  FAIL  {url}: {e:#}");
            }
        }
    }

    if failures > 0 {
        bail!("{failures} url+sha pair(s) failed verification");
    }
    if verified == 0 {
        bail!(
            "no url lines referenced /vector/{version}/ in formula ({skipped} url+sha pair(s) skipped)"
        );
    }
    Ok(format!(
        "formula at version {version}, {verified} url+sha pair(s) verified"
    ))
}

fn extract_version(formula: &str) -> Option<String> {
    for line in formula.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("version \"")
            && let Some(end) = rest.find('"')
        {
            return Some(rest[..end].to_string());
        }
    }
    None
}

// Walk the formula line by line. When we see a `url "..."` line, pair it with the first
// subsequent `sha256 "..."` line (which is the convention Homebrew formulas follow).
fn extract_url_sha_pairs(formula: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending_url: Option<String> = None;
    for line in formula.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("url \"") {
            if let Some(end) = rest.find('"') {
                pending_url = Some(rest[..end].to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("sha256 \"")
            && let Some(end) = rest.find('"')
            && let Some(url) = pending_url.take()
        {
            pairs.push((url, rest[..end].to_string()));
        }
    }
    pairs
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn download_and_digest(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    let mut resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    let mut hasher = Sha256::new();
    resp.copy_to(&mut hasher)
        .with_context(|| format!("streaming {url}"))?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{extract_url_sha_pairs, extract_version, is_hex64};

    const SAMPLE: &str = r#"class Vector < Formula
  version "0.55.0"

  on_macos do
    on_intel do
      url "https://packages.timber.io/vector/0.50.0/vector-0.50.0-x86_64-apple-darwin.tar.gz" # x86_64 url
      sha256 "14b7525b9fda86856e24ac9f52035852ae4168511709080d8081ad9f01f3dec4" # x86_64 sha256
    end

    on_arm do
      url "https://packages.timber.io/vector/0.55.0/vector-0.55.0-arm64-apple-darwin.tar.gz" # arm64 url
      sha256 "0691862ffa7c1135f0be5258ea34e3edf11288cc192bb67a3cd8d8cad914e8c3" # arm64 sha256
    end
  end
end
"#;

    #[test]
    fn extracts_version() {
        assert_eq!(extract_version(SAMPLE).as_deref(), Some("0.55.0"));
    }

    #[test]
    fn extracts_url_sha_pairs() {
        let pairs = extract_url_sha_pairs(SAMPLE);
        assert_eq!(pairs.len(), 2);
        assert!(pairs[0].0.contains("0.50.0"));
        assert_eq!(
            pairs[0].1,
            "14b7525b9fda86856e24ac9f52035852ae4168511709080d8081ad9f01f3dec4"
        );
        assert!(pairs[1].0.contains("0.55.0"));
        assert_eq!(
            pairs[1].1,
            "0691862ffa7c1135f0be5258ea34e3edf11288cc192bb67a3cd8d8cad914e8c3"
        );
    }

    #[test]
    fn hex64_validator() {
        assert!(is_hex64(
            "0691862ffa7c1135f0be5258ea34e3edf11288cc192bb67a3cd8d8cad914e8c3"
        ));
        assert!(!is_hex64("short"));
        assert!(!is_hex64(
            "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"
        ));
    }
}
