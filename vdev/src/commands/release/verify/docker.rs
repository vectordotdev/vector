use anyhow::{Context as _, Result, bail};
use serde_json::Value;

use super::{resolve_version, util};

// Repos that `publish-docker` in `.github/workflows/publish.yml` pushes to.
//
// Each tuple is `(human-readable label, registry host, image repo)`. The label is what we print;
// the other two are what we pass to the registry v2 API.
const REGISTRIES: &[Registry] = &[
    Registry {
        label: "docker.io",
        auth: "https://auth.docker.io/token?service=registry.docker.io&scope=repository:timberio/vector:pull",
        manifest_host: "registry-1.docker.io",
        repo: "timberio/vector",
    },
    Registry {
        label: "ghcr.io",
        auth: "https://ghcr.io/token?service=ghcr.io&scope=repository:vectordotdev/vector:pull",
        manifest_host: "ghcr.io",
        repo: "vectordotdev/vector",
    },
];

// Image variants the release docker script builds (see `scripts/build-docker.sh`). Each variant
// pushes under the version tag plus the `0.Y.X`, `0.X`, and `latest` aliases.
//
// `alpine` and `debian` ship all four platforms. `distroless-static` and `distroless-libc` skip
// `linux/arm/v6` because no upstream distroless base exists for that platform (see
// `SUPPORTED_PLATFORMS` in `scripts/build-docker.sh`).
const VARIANTS: &[Variant] = &[
    Variant {
        name: "alpine",
        platforms: ALL_PLATFORMS,
    },
    Variant {
        name: "debian",
        platforms: ALL_PLATFORMS,
    },
    Variant {
        name: "distroless-static",
        platforms: DISTROLESS_PLATFORMS,
    },
    Variant {
        name: "distroless-libc",
        platforms: DISTROLESS_PLATFORMS,
    },
];

const ALL_PLATFORMS: &[Platform] = &[
    Platform::LINUX_AMD64,
    Platform::LINUX_ARM64,
    Platform::LINUX_ARM_V7,
    Platform::LINUX_ARM_V6,
];

const DISTROLESS_PLATFORMS: &[Platform] = &[
    Platform::LINUX_AMD64,
    Platform::LINUX_ARM64,
    Platform::LINUX_ARM_V7,
];

const ACCEPT_MANIFEST_LIST: &str = "application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json";

struct Registry {
    label: &'static str,
    auth: &'static str,
    manifest_host: &'static str,
    repo: &'static str,
}

struct Variant {
    name: &'static str,
    platforms: &'static [Platform],
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Platform {
    display: &'static str,
    os: &'static str,
    architecture: &'static str,
    variant: Option<&'static str>,
}

impl Platform {
    const LINUX_AMD64: Self = Self {
        display: "linux/amd64",
        os: "linux",
        architecture: "amd64",
        variant: None,
    };
    const LINUX_ARM64: Self = Self {
        display: "linux/arm64",
        os: "linux",
        architecture: "arm64",
        variant: None,
    };
    const LINUX_ARM_V7: Self = Self {
        display: "linux/arm/v7",
        os: "linux",
        architecture: "arm",
        variant: Some("v7"),
    };
    const LINUX_ARM_V6: Self = Self {
        display: "linux/arm/v6",
        os: "linux",
        architecture: "arm",
        variant: Some("v6"),
    };

    fn matches(self, os: &str, architecture: &str, variant: Option<&str>) -> bool {
        self.os == os && self.architecture == architecture && self.variant == variant
    }
}

/// Verify Vector Docker images on Docker Hub and ghcr.io.
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
    let client = util::client()?;

    let aliases = tag_aliases(version)?;
    info!(
        "Checking Vector {version} Docker images (aliases: {})",
        aliases.join(", "),
    );

    let mut total = 0usize;
    let mut failures = 0usize;

    for registry in REGISTRIES {
        println!("  {}/{}:", registry.label, registry.repo);
        let token = fetch_token(&client, registry)
            .with_context(|| format!("fetching auth token for {}", registry.label))?;

        for alias in &aliases {
            for variant in VARIANTS {
                total += 1;
                let tag = format!("{alias}-{}", variant.name);
                match check_tag(&client, registry, &token, &tag, variant.platforms) {
                    Ok(covered) => {
                        println!("    {tag:<36} OK    [{}]", covered.join(", "));
                    }
                    Err(e) => {
                        failures += 1;
                        println!("    {tag:<36} FAIL  {e:#}");
                    }
                }
            }
        }
    }

    if failures > 0 {
        bail!("{failures}/{total} tag(s) missing or incomplete");
    }
    Ok(format!(
        "{total}/{total} tags OK across {} registries",
        REGISTRIES.len(),
    ))
}

// Expected release tag aliases, in the same order that `scripts/build-docker.sh` pushes them.
//
// For a semver release like `0.55.0` we expect `0.55.0`, `0.55.X`, `0.X`, and `latest`.
// Anything outside `MAJOR.MINOR.PATCH` (e.g. a pre-release suffix) is rejected because the
// release script only handles that shape.
fn tag_aliases(version: &str) -> Result<Vec<String>> {
    let parts: Vec<&str> = version.split('.').collect();
    let [major, minor, patch] = parts.as_slice() else {
        bail!(
            "expected version in MAJOR.MINOR.PATCH form, got {version:?} \
             (release-docker.sh only publishes tags for that shape)",
        );
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if [*major, *minor, *patch].iter().any(|s| !all_digits(s)) {
        bail!(
            "expected version in MAJOR.MINOR.PATCH form with numeric segments, got {version:?} \
             (release-docker.sh does not publish pre-release tags)",
        );
    }
    Ok(vec![
        version.to_string(),
        format!("{major}.{minor}.X"),
        format!("{major}.X"),
        "latest".to_string(),
    ])
}

fn fetch_token(client: &reqwest::blocking::Client, registry: &Registry) -> Result<String> {
    let body: Value = client
        .get(registry.auth)
        .send()
        .with_context(|| format!("GET {}", registry.auth))?
        .error_for_status()
        .with_context(|| format!("GET {}", registry.auth))?
        .json()
        .with_context(|| format!("parsing token JSON from {}", registry.auth))?;

    // Both docker.io and ghcr.io return `{"token": "..."}`; ghcr.io additionally includes
    // `access_token` with the same value. Only `token` is guaranteed.
    body.get("token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no `token` field in response from {}", registry.auth))
}

// Returns the list of expected platforms that were found in the manifest list, in the
// canonical order defined by `expected`. Errors if any expected platform is missing.
fn check_tag(
    client: &reqwest::blocking::Client,
    registry: &Registry,
    token: &str,
    tag: &str,
    expected: &[Platform],
) -> Result<Vec<&'static str>> {
    let url = format!(
        "https://{}/v2/{}/manifests/{tag}",
        registry.manifest_host, registry.repo,
    );
    let response = client
        .get(&url)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, ACCEPT_MANIFEST_LIST)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;

    let body: Value = response
        .json()
        .with_context(|| format!("parsing manifest JSON from {url}"))?;

    let manifests = body
        .get("manifests")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{url} returned a single-arch manifest (no `manifests` array); \
             expected a manifest list / OCI image index",
            )
        })?;

    let mut covered = Vec::with_capacity(expected.len());
    let mut missing = Vec::new();
    for platform in expected {
        if manifest_covers(manifests, *platform) {
            covered.push(platform.display);
        } else {
            missing.push(platform.display);
        }
    }

    if missing.is_empty() {
        Ok(covered)
    } else {
        bail!("missing platform(s): {}", missing.join(", "));
    }
}

fn manifest_covers(manifests: &[Value], expected: Platform) -> bool {
    manifests.iter().any(|entry| {
        let Some(plat) = entry.get("platform") else {
            return false;
        };
        let os = plat.get("os").and_then(Value::as_str).unwrap_or("");
        let architecture = plat
            .get("architecture")
            .and_then(Value::as_str)
            .unwrap_or("");
        let variant = plat.get("variant").and_then(Value::as_str);
        expected.matches(os, architecture, variant)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Platform, manifest_covers, tag_aliases};

    #[test]
    fn aliases_cover_version_series() {
        assert_eq!(
            tag_aliases("0.55.0").unwrap(),
            vec!["0.55.0", "0.55.X", "0.X", "latest"],
        );
        assert_eq!(
            tag_aliases("1.2.3").unwrap(),
            vec!["1.2.3", "1.2.X", "1.X", "latest"],
        );
    }

    #[test]
    fn aliases_reject_non_semver() {
        assert!(tag_aliases("0.55").is_err());
        assert!(tag_aliases("0.55.0-rc1").is_err());
        assert!(tag_aliases("nightly").is_err());
    }

    #[test]
    fn manifest_covers_matches_arm_variants() {
        let manifests = vec![
            json!({"platform": {"os": "linux", "architecture": "amd64"}}),
            json!({"platform": {"os": "linux", "architecture": "arm64"}}),
            json!({"platform": {"os": "linux", "architecture": "arm", "variant": "v7"}}),
        ];
        assert!(manifest_covers(&manifests, Platform::LINUX_AMD64));
        assert!(manifest_covers(&manifests, Platform::LINUX_ARM64));
        assert!(manifest_covers(&manifests, Platform::LINUX_ARM_V7));
        assert!(!manifest_covers(&manifests, Platform::LINUX_ARM_V6));
    }

    #[test]
    fn manifest_covers_ignores_attestation_entries() {
        // buildx pushes attestation manifests with architecture/os = "unknown"; they
        // must not satisfy any expected platform.
        let manifests = vec![json!({"platform": {"os": "unknown", "architecture": "unknown"}})];
        assert!(!manifest_covers(&manifests, Platform::LINUX_AMD64));
    }
}
