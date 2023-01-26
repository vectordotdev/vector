use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::config::Environment;
use crate::platform;

const CONFIG_FILE: &str = "config.json";

pub struct EnvsDir {
    path: PathBuf,
}

impl EnvsDir {
    pub fn new(integration: &str) -> Self {
        let path = [
            platform::data_dir().as_path(),
            Path::new("integration"),
            Path::new("envs"),
            Path::new(integration),
        ]
        .iter()
        .collect();
        Self { path }
    }

    pub fn exists(&self, environment: &str) -> bool {
        self.path.join(environment).is_dir()
    }

    pub fn list_active(&self) -> Result<HashSet<String>> {
        let mut environments = HashSet::new();
        if self.path.is_dir() {
            for entry in self
                .path
                .read_dir()
                .with_context(|| format!("failed to read directory {:?}", self.path))?
            {
                let entry = entry.with_context(|| {
                    format!("failed to read directory entry in {:?}", self.path)
                })?;
                if entry.path().is_dir() {
                    environments.insert(entry.file_name().into_string().unwrap_or_else(|entry| {
                        panic!("Invalid directory entry in {:?}: {entry:?}", self.path)
                    }));
                }
            }
        }
        Ok(environments)
    }

    pub fn save(&self, environment: &str, config: &Environment) -> Result<()> {
        let mut path = self.path.join(environment);
        if !path.is_dir() {
            fs::create_dir_all(&path)
                .with_context(|| format!("failed to create directory {path:?}"))?;
        }

        let config = serde_json::to_string(&config)?;
        path.push(CONFIG_FILE);
        fs::write(&path, config).with_context(|| format!("failed to write file {path:?}"))
    }

    pub fn read_config(&self, environment: &str) -> Result<Environment> {
        let mut config_file = self.path.join(environment);
        config_file.push(CONFIG_FILE);

        let json = fs::read_to_string(&config_file)
            .with_context(|| format!("failed to write file {config_file:?}"))?;
        serde_json::from_str(&json).with_context(|| format!("invalid contents in {config_file:?}"))
    }

    pub fn remove(&self, environment: &str) -> Result<()> {
        let env_path = self.path.join(environment);
        if env_path.is_dir() {
            fs::remove_dir_all(&env_path)
                .with_context(|| format!("failed to remove directory {env_path:?}"))?;
        }

        Ok(())
    }
}
