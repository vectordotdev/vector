use std::time::Duration;

use anyhow::{Context as _, Result, bail};

use super::{VerifyOutcome, resolve_version};

const DEFAULT_BASE_URL: &str = "https://vector.dev";

/// Verify `vector.dev` serves the release page and docs reference the release.
///
/// Checks that Hugo has rebuilt the site post-release: the per-release page at
/// `/releases/<ver>/` is live, the releases index lists it, and the download page
/// points at `<ver>`.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Version to verify (e.g. `0.55.0`). Defaults to the most recent `v*` git tag.
    version: Option<String>,

    /// Base URL of the website.
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

// Each check is a (label, url, required-substring). The substring must appear in the
// body of the 200 response; we don't parse the HTML since a plain `contains` is enough
// to distinguish "Hugo has rebuilt with the new version" from "stale site".
fn verify_inner(version: &str, base_url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    info!("Checking Vector {version} on {base_url}");

    let release_page = format!("{base_url}/releases/{version}/");
    let releases_index = format!("{base_url}/releases/");
    let download_page = format!("{base_url}/download/");
    // The releases index should link to the per-release page, not merely mention the
    // version string (which might appear in unrelated changelog blurbs on the page).
    let release_link = format!("/releases/{version}/");

    let checks: [(&str, &str, &str); 3] = [
        ("release page", release_page.as_str(), version),
        (
            "releases index",
            releases_index.as_str(),
            release_link.as_str(),
        ),
        ("download page", download_page.as_str(), version),
    ];

    let total = checks.len();
    let mut failures = 0usize;
    for (label, url, needle) in checks {
        match check_page(&client, url, needle) {
            Ok(CheckResult { status, bytes }) => {
                println!("  {label:<15} OK    {status} {url} ({bytes} bytes, found {needle:?})");
            }
            Err(e) => {
                failures += 1;
                println!("  {label:<15} FAIL  {url} -- {e:#}");
            }
        }
    }

    if failures > 0 {
        bail!("{failures}/{total} website pages missing or stale for {version}");
    }
    Ok(format!("{total}/{total} pages OK"))
}

struct CheckResult {
    status: u16,
    bytes: usize,
}

fn check_page(client: &reqwest::blocking::Client, url: &str, needle: &str) -> Result<CheckResult> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("fetching {url}"))?
        .error_for_status()
        .with_context(|| format!("fetching {url}"))?;
    let status = resp.status().as_u16();
    let body = resp
        .text()
        .with_context(|| format!("reading body of {url}"))?;
    let bytes = body.len();
    if !body.contains(needle) {
        bail!("body did not contain {needle:?} (status {status}, {bytes} bytes)");
    }
    Ok(CheckResult { status, bytes })
}

#[cfg(test)]
mod tests {
    #[test]
    fn needle_contains_is_enough() {
        // Sanity: the "search for a substring" rule we rely on in `check_page` is the
        // plain `str::contains`, which is case-sensitive. Document that expectation
        // here so a future refactor doesn't silently loosen it.
        let body = "<a href=\"/releases/0.55.0/\">0.55.0</a>";
        assert!(body.contains("0.55.0"));
        assert!(body.contains("/releases/0.55.0/"));
        assert!(!body.contains("0.55.1"));
    }
}
