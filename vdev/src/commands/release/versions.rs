use std::fs;

use anyhow::{Context, Result, ensure};
use semver::{Prerelease, Version};
use toml_edit::DocumentMut;

use super::ensure_stable;
use crate::utils::git;

/// Version overrides shared by local release preparation and workflow preflight.
#[derive(clap::Args, Debug)]
pub(super) struct VersionOptions {
    /// Vector release version; defaults to Cargo.toml's development minor without -dev.
    #[arg(long)]
    version: Option<Version>,
    /// Released VRL version; defaults to the latest stable tag in the VRL repository.
    #[arg(long)]
    pub(super) vrl_version: Option<Version>,
}

impl VersionOptions {
    pub(super) fn resolve_vector(&self) -> Result<Version> {
        preparation_version(&current_cargo_version()?, self.version.clone())
    }

    pub(super) fn resolve_vrl(&self, existing_pin: Option<Version>) -> Result<Version> {
        let version = match self.vrl_version.clone().or(existing_pin) {
            Some(version) => version,
            None => latest_vrl_version()?,
        };
        ensure_stable(&version, "VRL version")?;
        Ok(version)
    }
}

fn preparation_version(current: &Version, requested: Option<Version>) -> Result<Version> {
    let version = requested.unwrap_or_else(|| Version::new(current.major, current.minor, 0));
    ensure_stable(&version, "release version")?;
    let expected_current = development_version(&version)?;
    ensure!(
        *current == expected_current,
        "expected Cargo.toml version {expected_current}, found {current}"
    );
    Ok(version)
}

fn latest_vrl_version() -> Result<Version> {
    let pattern = regex::Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+$").unwrap();
    let tag = git::highest_release_tag("https://github.com/vectordotdev/vrl.git", &pattern)
        .context("failed to fetch the latest stable VRL tag")?;
    parse_stable_version(tag.trim_start_matches('v'), "VRL version")
}

pub(super) fn pinned_vrl_version(manifest: &toml::Value) -> Result<Version> {
    let version = manifest
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("vrl"))
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str)
        .context("Cargo.toml must pin VRL to a released registry version")?;
    parse_stable_version(version, "VRL version")
}

pub(super) fn parse_stable_version(value: &str, label: &str) -> Result<Version> {
    let version = Version::parse(value).with_context(|| format!("invalid {label}: {value}"))?;
    ensure_stable(&version, label)?;
    Ok(version)
}

pub(super) fn development_version(version: &Version) -> Result<Version> {
    ensure_stable(version, "release version")?;
    let mut development = version.clone();
    development.pre = Prerelease::new("dev")?;
    Ok(development)
}

pub(super) fn current_cargo_version() -> Result<Version> {
    parse_cargo_version(&fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?)
}

pub(super) fn parse_cargo_version(contents: &str) -> Result<Version> {
    let doc = contents
        .parse::<DocumentMut>()
        .context("failed to parse Cargo.toml")?;
    let version = doc["package"]["version"]
        .as_str()
        .context("Cargo.toml package.version must be a string")?;
    Version::parse(version).context("Cargo.toml package.version is not a semantic version")
}

#[cfg(test)]
mod tests {
    use super::{VersionOptions, parse_cargo_version, preparation_version};
    use indoc::indoc;

    #[test]
    fn preparation_version_defaults_to_the_development_minor() {
        for version in ["0.59.0", "0.61.0", "1.2.0"] {
            let current = format!("{version}-dev").parse().unwrap();
            assert_eq!(
                preparation_version(&current, None).unwrap(),
                version.parse().unwrap()
            );
        }
    }

    #[test]
    fn preparation_version_validates_explicit_versions() {
        let current = "0.61.0-dev".parse().unwrap();
        assert_eq!(
            preparation_version(&current, Some("0.61.0".parse().unwrap())).unwrap(),
            "0.61.0".parse().unwrap()
        );
        for version in ["0.60.0", "0.62.0", "0.61.1", "0.61.0-rc.1", "0.61.0+build"] {
            assert!(
                preparation_version(&current, Some(version.parse().unwrap())).is_err(),
                "accepted {version}"
            );
        }
    }

    #[test]
    fn preparation_version_requires_a_minor_development_version() {
        for version in ["0.61.0", "0.61.1-dev", "0.61.0-rc.1", "0.61.0-dev+build"] {
            assert!(
                preparation_version(&version.parse().unwrap(), None).is_err(),
                "accepted {version}"
            );
        }
    }

    #[test]
    fn parses_package_version() {
        let cargo_toml = indoc! {r#"
            [package]
            name = "vector"
            version = "0.59.0-dev"
        "#};
        assert_eq!(
            parse_cargo_version(cargo_toml).expect("valid Cargo.toml"),
            "0.59.0-dev".parse().expect("valid version")
        );
    }

    #[test]
    fn explicit_patch_versions_remain_supported_for_local_preparation() {
        let version = "0.61.1".parse().unwrap();
        assert_eq!(
            preparation_version(&"0.61.1-dev".parse().unwrap(), Some(version)).unwrap(),
            "0.61.1".parse().unwrap()
        );
    }

    #[test]
    fn vrl_override_takes_precedence_over_an_existing_pin() {
        let options = VersionOptions {
            version: None,
            vrl_version: Some("0.28.1".parse().unwrap()),
        };
        assert_eq!(
            options
                .resolve_vrl(Some("0.28.0".parse().unwrap()))
                .unwrap(),
            "0.28.1".parse().unwrap()
        );
    }
}
