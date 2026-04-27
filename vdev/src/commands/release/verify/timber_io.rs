use anyhow::{Context as _, Result, bail};

use super::{resolve_version, util};

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
// Versionless-stripped form (`${file/-$VERSION_EXACT/}` in release-s3.sh): a website
// redirect that `release-s3.sh` creates in *every* alias path (exact, `{major}.{minor}.X`,
// `{major}.X`, `latest`). Must resolve to 200 after following the redirect.
const ALIAS_PROBE_STRIPPED: &str = "vector-x86_64-unknown-linux-gnu.tar.gz";
// `latest`-substitution form (`${file/$VERSION_EXACT/latest}` in release-s3.sh): only
// created under `/vector/latest/`.
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
        let summary = verify_with_url(&version, &self.url)?;
        println!("OK: {summary}");
        Ok(())
    }
}

pub fn verify(version: &str) -> Result<String> {
    verify_with_url(version, DEFAULT_BASE_URL)
}

fn verify_with_url(version: &str, base_url: &str) -> Result<String> {
    info!("Checking Vector {version} at {base_url}/{version}/");

    // HEADs are quick; SHA256 GETs stream multi-MB bodies and need a generous timeout.
    let head_client = util::client()?;
    let stream_client = util::stream_client()?;

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
        match util::head_size(client, &url) {
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

    let mut failures = 0usize;
    for alias in &aliases {
        // Versioned filename served directly (200) at every alias path.
        let url = format!("{base_url}/{alias}/{versioned_alias_file}");
        failures += report_alias(alias, &versioned_alias_file, util::head_size(client, &url));

        // Versionless-stripped redirect, created by `release-s3.sh` in every alias path.
        let stripped_url = format!("{base_url}/{alias}/{ALIAS_PROBE_STRIPPED}");
        failures += report_alias(alias, ALIAS_PROBE_STRIPPED, util::head_size(client, &stripped_url));
    }

    // The `latest`-named alias (e.g. `vector-latest-...`) is a website-redirect -> 200,
    // only present under `/vector/latest/`.
    let url = format!("{base_url}/latest/{ALIAS_PROBE_LATEST_TEMPLATE}");
    failures += report_alias("latest", ALIAS_PROBE_LATEST_TEMPLATE, util::head_size(client, &url));

    Ok(failures)
}

fn report_alias(alias: &str, filename: &str, result: Result<u64>) -> usize {
    match result {
        Ok(size) => {
            println!("  {alias:<8} OK    {filename} ({size} bytes)");
            0
        }
        Err(e) => {
            println!("  {alias:<8} FAIL  {filename}: {e:#}");
            1
        }
    }
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
    let sums = util::parse_sha256sums(&util::fetch_text(client, &sha_url)?);

    let mut ok = 0usize;
    let mut fail = 0usize;
    for template in CHECKSUM_SAMPLES {
        let filename = template.replace("{v}", version);
        let Some(expected_hex) = sums.get(&filename) else {
            fail += 1;
            println!("  checksum FAIL  {filename}: not listed in SHA256SUMS");
            continue;
        };
        let url = format!("{base_url}/{version}/{filename}");
        match util::stream_sha256(client, &url) {
            Ok(actual) if actual.eq_ignore_ascii_case(expected_hex) => {
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
    use super::parse_major_minor;

    #[test]
    fn parses_major_minor_basic() {
        assert_eq!(parse_major_minor("0.55.0").unwrap(), ("0", "55"));
        assert_eq!(parse_major_minor("1.2.3-pre").unwrap(), ("1", "2"));
        assert!(parse_major_minor("1").is_err());
        assert!(parse_major_minor("").is_err());
    }
}
