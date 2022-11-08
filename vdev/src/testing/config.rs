use anyhow::{bail, Context, Result};
use cached::proc_macro::cached;
use globset::{Glob, GlobSetBuilder};
use hashlink::LinkedHashMap;
use itertools::{self, Itertools};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::iter;
use std::path::PathBuf;

#[derive(Deserialize, Debug)]
pub struct RustToolchainRootConfig {
    pub(crate) toolchain: RustToolchainConfig,
}

#[derive(Deserialize, Debug)]
pub struct RustToolchainConfig {
    pub(crate) channel: String,
}

impl RustToolchainConfig {
    pub fn parse(repo_path: &String) -> Result<Self> {
        let config_file: PathBuf = [repo_path, "rust-toolchain.toml"].iter().collect();
        let contents = fs::read_to_string(&config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let config: RustToolchainRootConfig = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_file.display()))?;

        Ok(config.toolchain)
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct IntegrationTestConfig {
    on: Vec<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: Option<BTreeMap<String, String>>,
    matrix: Vec<LinkedHashMap<String, Vec<String>>>,
}

impl IntegrationTestConfig {
    fn parse_file(config_file: &PathBuf) -> Result<Self> {
        let contents = fs::read_to_string(config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let config: IntegrationTestConfig = toml::from_str(&contents).with_context(|| {
            format!(
                "failed to parse integration test configuration file {}",
                config_file.display()
            )
        })?;

        Ok(config)
    }

    pub fn triggered(&self, changed_files: Vec<String>) -> Result<bool> {
        let mut builder = GlobSetBuilder::new();
        for glob in self.on.iter() {
            builder.add(Glob::new(&glob)?);
        }
        let set = builder.build()?;

        Ok(changed_files
            .iter()
            .any(|changed_file| set.is_match(changed_file)))
    }

    pub fn environments(&self) -> LinkedHashMap<String, LinkedHashMap<String, String>> {
        let mut environments = LinkedHashMap::new();

        for matrix in self.matrix.iter() {
            for product in matrix.values().multi_cartesian_product() {
                let mut config = LinkedHashMap::new();
                for (variable, value) in iter::zip(matrix.keys(), product.iter()) {
                    config.insert(variable.to_string(), value.to_string());
                }

                environments.insert(product.iter().join("-"), config);
            }
        }

        environments
    }

    pub fn locate_source(root: &String, integration: &str) -> Result<PathBuf> {
        let test_dir: PathBuf = [&root, "scripts", "integration", integration]
            .iter()
            .collect();
        if !test_dir.is_dir() {
            bail!("unknown integration: {}", integration);
        }

        Ok(test_dir)
    }

    pub fn from_source(test_dir: &PathBuf) -> Result<Self> {
        parse_integration_test_config_file(test_dir.join("test.toml"))
    }

    pub fn collect_all(root: &String) -> Result<BTreeMap<String, Self>> {
        let mut configs = BTreeMap::new();
        let tests_dir: PathBuf = [&root, "scripts", "integration"].iter().collect();
        for entry in tests_dir.read_dir()? {
            if let Ok(entry) = entry {
                if !entry.path().is_dir() {
                    continue;
                }

                let config_file: PathBuf = [&entry.path().to_str().unwrap(), "test.toml"]
                    .iter()
                    .collect();
                let config = parse_integration_test_config_file(config_file)?;
                configs.insert(entry.file_name().into_string().unwrap(), config);
            }
        }

        Ok(configs)
    }
}

#[cached(result = true)]
fn parse_integration_test_config_file(config_file: PathBuf) -> Result<IntegrationTestConfig> {
    IntegrationTestConfig::parse_file(&config_file)
}
