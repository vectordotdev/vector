//! Semantic-version helpers shared across vdev commands.

use anyhow::{Context, Result, bail};
use semver::Version;

/// Parse a version, accepting the full semver grammar including prerelease
/// (`0.59.0-rc.1`) and build (`0.59.0+build`) segments. Semver ordering
/// guarantees a prerelease sorts before its own release, so comparisons
/// stay correct for non-stable versions.
pub fn parse(s: &str) -> Result<Version> {
    Version::parse(s.trim()).with_context(|| format!("invalid semver version '{s}'"))
}

/// Ensure `candidate` is strictly newer than `current`.
///
/// Errors name both versions and the relationship so the failure is
/// actionable in CI logs (e.g. pinned chart older than the one already
/// rendered into the repo).
pub fn ensure_newer(candidate: &Version, current: &Version, what: &str) -> Result<()> {
    if candidate <= current {
        bail!("{what} version {candidate} is not newer than {current}; refusing to continue");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        parse(s).unwrap()
    }

    #[test]
    fn parse_accepts_full_semver() {
        assert_eq!(v("0.58.0").to_string(), "0.58.0");
        assert_eq!(v(" 1.2.3 ").to_string(), "1.2.3");
        assert_eq!(v("0.59.0-rc.1").pre.as_str(), "rc.1");
        assert_eq!(v("0.59.0+build.2").build.as_str(), "build.2");
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse("0.58").is_err());
        assert!(parse("latest").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn ensure_newer_accepts_only_strictly_newer() {
        let current = v("0.58.0");
        assert!(ensure_newer(&v("0.59.0"), &current, "chart").is_ok());
        assert!(ensure_newer(&v("0.58.1"), &current, "chart").is_ok());
        assert!(ensure_newer(&v("0.58.0"), &current, "chart").is_err());
        assert!(ensure_newer(&v("0.57.9"), &current, "chart").is_err());
    }

    #[test]
    fn ensure_newer_orders_prereleases_by_semver() {
        let current = v("0.58.0");
        assert!(ensure_newer(&v("0.58.1-rc.1"), &current, "chart").is_ok());
        // A prerelease of a not-yet-released newer version counts as newer.
        assert!(ensure_newer(&v("0.59.0-rc.1"), &current, "chart").is_ok());
        // The stable release outranks its own prerelease.
        let pre = v("0.59.0-rc.1");
        assert!(ensure_newer(&v("0.59.0"), &pre, "chart").is_ok());
        assert!(ensure_newer(&v("0.59.0-rc.1"), &pre, "chart").is_err());
    }
}
