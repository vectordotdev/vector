use std::fs;
use std::process::{Command, Output};

use anyhow::{Context as _, Result};
use serde::Deserialize;

/// Read the version string from `Cargo.toml`
pub fn read_version() -> Result<String> {
    #[derive(Deserialize)]
    struct Package {
        version: String,
    }
    #[derive(Deserialize)]
    struct CargoToml {
        package: Package,
    }

    let text = fs::read_to_string("Cargo.toml").context("Could not read `Cargo.toml`")?;
    toml::from_str::<CargoToml>(&text)
        .context("Invalid contents in `Cargo.toml`")
        .map(|cargo| cargo.package.version)
}

pub fn git_head() -> Result<Output> {
    Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .output()
        .context("Could not execute `git`")
}

/// Calculate the release channel from `git describe`
pub fn release_channel() -> Result<&'static str> {
    git_head().map(|output| {
        if output.status.success() {
            "latest"
        } else {
            "nightly"
        }
    })
}
