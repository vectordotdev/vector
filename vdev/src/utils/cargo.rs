//! Cargo.toml parsing and version utilities

use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result};
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

/// Use the version provided by env vars or default to reading from `Cargo.toml`.
pub fn get_version() -> Result<String> {
    std::env::var("VERSION")
        .or_else(|_| std::env::var("VECTOR_VERSION"))
        .or_else(|_| read_version())
}
