use anyhow::{Context as _, Result, bail};

use super::{resolve_version, util};

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
        let summary = verify_with_url(&version, &self.url)?;
        println!("OK: {summary}");
        Ok(())
    }
}

pub fn verify(version: &str) -> Result<String> {
    verify_with_url(version, DEFAULT_BASE_URL)
}

fn verify_with_url(version: &str, base_url: &str) -> Result<String> {
    let client = util::client()?;

    info!("Checking Vector {version} on {base_url}");

    let total = 3usize;
    let mut failures = 0usize;

    // Per-release page: `/releases/<ver>/` must 200 and mention `<ver>`.
    let release_page = format!("{base_url}/releases/{version}/");
    failures += report(
        "release page",
        &release_page,
        fetch_page_containing(&client, &release_page, version),
    );

    // Releases index: must include a link to the per-release page, not just the bare
    // version string (which appears in unrelated changelog blurbs on the page).
    let releases_index = format!("{base_url}/releases/");
    let release_link = format!("/releases/{version}/");
    failures += report(
        "releases index",
        &releases_index,
        fetch_page_containing(&client, &releases_index, &release_link),
    );

    // Download page: the first non-nightly version in the selector must equal `<ver>`.
    // See `website/layouts/partials/download/version-selector.html`: the range runs over
    // `site.Data.docs.versions` in order, and `$latest := index $versions 0`, so the
    // first non-nightly `setVersion('...')` button in the HTML corresponds to $latest.
    // A plain `contains(version)` match is not enough — every release in `versions.yaml`
    // emits its own `setVersion('X')` button, so an older release page can contain the
    // new version string before Hugo rebuilds.
    let download_page = format!("{base_url}/download/");
    failures += report(
        "download page",
        &download_page,
        check_download_page(&client, &download_page, version),
    );

    if failures > 0 {
        bail!("{failures}/{total} website pages missing or stale for {version}");
    }
    Ok(format!("{total}/{total} pages OK"))
}

// Pretty-print a per-check line and return 1 if the check failed, 0 otherwise.
fn report(label: &str, url: &str, result: Result<usize>) -> usize {
    match result {
        Ok(bytes) => {
            println!("  {label:<15} OK    {url} ({bytes} bytes)");
            0
        }
        Err(e) => {
            println!("  {label:<15} FAIL  {url} -- {e:#}");
            1
        }
    }
}

fn fetch_page_containing(
    client: &reqwest::blocking::Client,
    url: &str,
    needle: &str,
) -> Result<usize> {
    let body = util::fetch_text(client, url)?;
    if !body.contains(needle) {
        bail!("body did not contain {needle:?} ({} bytes)", body.len());
    }
    Ok(body.len())
}

fn check_download_page(
    client: &reqwest::blocking::Client,
    url: &str,
    version: &str,
) -> Result<usize> {
    let body = util::fetch_text(client, url)?;
    let latest = latest_selector_version(&body).with_context(|| {
        format!("no `setVersion('<ver>')` button found on {url} (template changed?)")
    })?;
    if latest != version {
        bail!(
            "download page lists {latest:?} as the latest release, expected {version:?} (Hugo still stale?)"
        );
    }
    Ok(body.len())
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
