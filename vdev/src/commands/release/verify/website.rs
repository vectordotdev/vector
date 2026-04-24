use std::time::Duration;

use anyhow::{Context as _, Result, bail};

use super::{VerifyOutcome, resolve_version};

const DEFAULT_BASE_URL: &str = "https://vector.dev";

/// Verify `vector.dev` serves the release page and docs reference the release.
///
/// Checks that Hugo has rebuilt the site post-release: the per-release page at
/// `/releases/<ver>/` is live, the releases index lists it, and the download page
/// treats `<ver>` as the current release.
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

fn verify_inner(version: &str, base_url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    info!("Checking Vector {version} on {base_url}");

    let total = 3usize;
    let mut failures = 0usize;

    // Per-release page: `/releases/<ver>/` must 200 and mention `<ver>`.
    let release_page = format!("{base_url}/releases/{version}/");
    match fetch_page_containing(&client, &release_page, version) {
        Ok(PageResult { status, bytes }) => {
            println!("  release page    OK    {status} {release_page} ({bytes} bytes)");
        }
        Err(e) => {
            failures += 1;
            println!("  release page    FAIL  {release_page} -- {e:#}");
        }
    }

    // Releases index: must include a link to the per-release page, not just the bare
    // version string (which appears in unrelated changelog blurbs on the page).
    let releases_index = format!("{base_url}/releases/");
    let release_link = format!("/releases/{version}/");
    match fetch_page_containing(&client, &releases_index, &release_link) {
        Ok(PageResult { status, bytes }) => {
            println!("  releases index  OK    {status} {releases_index} ({bytes} bytes)");
        }
        Err(e) => {
            failures += 1;
            println!("  releases index  FAIL  {releases_index} -- {e:#}");
        }
    }

    // Download page: the first non-nightly version in the selector must equal `<ver>`.
    // See `website/layouts/partials/download/version-selector.html`: the range runs over
    // `site.Data.docs.versions` in order, and `$latest := index $versions 0`, so the
    // first non-nightly `setVersion('...')` button in the HTML corresponds to $latest.
    // A plain `contains(version)` match is not enough — every release in `versions.yaml`
    // emits its own `setVersion('X')` button, so an older release page can contain the
    // new version string before Hugo rebuilds.
    let download_page = format!("{base_url}/download/");
    match check_download_page(&client, &download_page, version) {
        Ok(PageResult { status, bytes }) => {
            println!("  download page   OK    {status} {download_page} ({bytes} bytes)");
        }
        Err(e) => {
            failures += 1;
            println!("  download page   FAIL  {download_page} -- {e:#}");
        }
    }

    if failures > 0 {
        bail!("{failures}/{total} website pages missing or stale for {version}");
    }
    Ok(format!("{total}/{total} pages OK"))
}

struct PageResult {
    status: u16,
    bytes: usize,
}

fn fetch_page_containing(
    client: &reqwest::blocking::Client,
    url: &str,
    needle: &str,
) -> Result<PageResult> {
    let (status, body) = fetch(client, url)?;
    if !body.contains(needle) {
        bail!(
            "body did not contain {needle:?} (status {status}, {} bytes)",
            body.len()
        );
    }
    Ok(PageResult {
        status,
        bytes: body.len(),
    })
}

fn check_download_page(
    client: &reqwest::blocking::Client,
    url: &str,
    version: &str,
) -> Result<PageResult> {
    let (status, body) = fetch(client, url)?;
    let latest = latest_selector_version(&body).with_context(|| {
        format!("no `setVersion('<ver>')` button found on {url} (template changed?)")
    })?;
    if latest != version {
        bail!(
            "download page lists {latest:?} as the latest release, expected {version:?} (Hugo still stale?)"
        );
    }
    Ok(PageResult {
        status,
        bytes: body.len(),
    })
}

fn fetch(client: &reqwest::blocking::Client, url: &str) -> Result<(u16, String)> {
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
    Ok((status, body))
}

// Scan the download page for the first `setVersion('X')` call whose argument is not
// `nightly` — that is, the first release version in the selector's range loop, which
// Hugo renders in the same order as `site.Data.docs.versions` (latest first).
fn latest_selector_version(body: &str) -> Option<&str> {
    const MARKER: &str = "setVersion('";
    let mut search_from = 0;
    while let Some(rel) = body[search_from..].find(MARKER) {
        let start = search_from + rel + MARKER.len();
        let end = start + body[start..].find('\'')?;
        let v = &body[start..end];
        if v != "nightly" {
            return Some(v);
        }
        search_from = end + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::latest_selector_version;

    #[test]
    fn finds_first_non_nightly_version() {
        let body = r#"
            <button @click="$store.global.setVersion('nightly'); open = false">nightly</button>
            <button @click="$store.global.setVersion('0.55.0'); open = false">0.55.0</button>
            <button @click="$store.global.setVersion('0.54.0'); open = false">0.54.0</button>
        "#;
        assert_eq!(latest_selector_version(body), Some("0.55.0"));
    }

    #[test]
    fn skips_nightly_even_when_it_appears_multiple_times() {
        let body = r#"setVersion('nightly') setVersion('nightly') setVersion('0.55.0')"#;
        assert_eq!(latest_selector_version(body), Some("0.55.0"));
    }

    #[test]
    fn returns_none_when_only_nightly_present() {
        let body = r#"setVersion('nightly')"#;
        assert_eq!(latest_selector_version(body), None);
    }

    #[test]
    fn returns_none_when_marker_absent() {
        assert_eq!(latest_selector_version(""), None);
    }

    #[test]
    fn detects_stale_download_page() {
        // When Hugo still thinks 0.54.0 is latest, `setVersion('0.54.0')` appears before
        // `setVersion('0.55.0')` — the probe should see 0.54.0 as the latest and flag it.
        let stale = r#"setVersion('nightly') setVersion('0.54.0') setVersion('0.55.0')"#;
        assert_eq!(latest_selector_version(stale), Some("0.54.0"));
    }
}
