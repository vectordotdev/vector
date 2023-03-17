use std::ffi::{OsStr, OsString};
use std::process::{Command, Output};
use std::{collections::BTreeMap, fmt::Debug, fs, io::ErrorKind, path::Path};

use anyhow::{Context as _, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct CargoTomlPackage {
    pub version: String,
}

/// The bits of the top-level `Cargo.toml` configuration that `vdev` uses to drive its features.
#[derive(Deserialize)]
pub struct CargoToml {
    pub package: CargoTomlPackage,
    pub features: BTreeMap<String, Value>,
}

impl CargoToml {
    pub fn load() -> Result<CargoToml> {
        let text = fs::read_to_string("Cargo.toml").context("Could not read `Cargo.toml`")?;
        toml::from_str::<CargoToml>(&text).context("Invalid contents in `Cargo.toml`")
    }
}

/// Read the version string from `Cargo.toml`
pub fn read_version() -> Result<String> {
    CargoToml::load().map(|cargo| cargo.package.version)
}

pub fn git_head() -> Result<Output> {
    Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .output()
        .context("Could not execute `git`")
}

pub fn git_short_hash() -> Result<Output> {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .context("Could not execute `git`")
}

// If in CI, it is necessary to adjust the permissions on the git repository
// as a precursor to running some git commands.
pub fn mark_safe_git_repo() {
    if let Ok(is_ci) = std::env::var("CI") {
        if is_ci == "true" {
            std::process::Command::new("git")
                .args([
                    "config",
                    "--global",
                    "--add",
                    "safe.directory",
                    "/git/vectordotdev/vector",
                ])
                .output()
                .context("Could not execute `git config --add safe.directory`")
                .unwrap();
        }
    }
}

pub fn get_mode() -> String {
    std::env::var("MODE").unwrap_or_else(|_| "custom".to_string())
}

pub fn exists(path: impl AsRef<Path> + Debug) -> Result<bool> {
    match fs::metadata(path.as_ref()) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context(format!("Could not stat {path:?}")),
    }
}

pub trait ChainArgs {
    fn chain_args<I: Into<OsString>>(&self, args: impl IntoIterator<Item = I>) -> Vec<OsString>;
    fn chain_arg(&self, arg: impl Into<OsString>) -> Vec<OsString> {
        self.chain_args([arg])
    }
}

impl<T: AsRef<OsStr>> ChainArgs for Vec<T> {
    fn chain_args<I: Into<OsString>>(&self, args: impl IntoIterator<Item = I>) -> Vec<OsString> {
        self.iter()
            .map(Into::into)
            .chain(args.into_iter().map(Into::into))
            .collect()
    }
}

impl<T: AsRef<OsStr>> ChainArgs for [T] {
    fn chain_args<I: Into<OsString>>(&self, args: impl IntoIterator<Item = I>) -> Vec<OsString> {
        self.iter()
            .map(Into::into)
            .chain(args.into_iter().map(Into::into))
            .collect()
    }
}
