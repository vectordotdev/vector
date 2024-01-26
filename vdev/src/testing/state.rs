use std::path::{Path, PathBuf};
use std::{fs, io::ErrorKind};

use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use super::config::Environment;
use crate::{platform, util};

static DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    [platform::data_dir(), Path::new("integration")]
        .into_iter()
        .collect()
});

pub struct EnvsDir {
    path: PathBuf,
}

#[derive(Deserialize, Serialize)]
pub struct State {
    pub active: String,
    pub config: Environment,
}

impl EnvsDir {
    pub fn new(integration: &str) -> Self {
        let config = format!("{integration}.json");
        let path = [&DATA_DIR, Path::new(&config)].iter().collect();
        Self { path }
    }

    /// Check if the named environment is active. If the current config could not be loaded or a
    /// different environment is active, an error is returned.
    pub fn check_active(&self, name: &str) -> Result<bool> {
        match self.active()? {
            None => Ok(false),
            Some(active) if active == name => Ok(true),
            Some(active) => Err(anyhow!(
                "Requested environment {name:?} does not match active one {active:?}"
            )),
        }
    }

    /// Return the currently active environment name.
    pub fn active(&self) -> Result<Option<String>> {
        self.load().map(|state| state.map(|config| config.active))
    }

    /// Load the currently active state data.
    pub fn load(&self) -> Result<Option<State>> {
        let json = match fs::read_to_string(&self.path) {
            Ok(json) => json,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).context(format!("Could not read state file {:?}", self.path))
            }
        };
        let state: State = serde_json::from_str(&json)
            .with_context(|| format!("Could not parse state file {:?}", self.path))?;
        Ok(Some(state))
    }

    pub fn save(&self, environment: &str, config: &Environment) -> Result<()> {
        let config = State {
            active: environment.into(),
            config: config.clone(),
        };
        let path = &*DATA_DIR;
        if !path.is_dir() {
            fs::create_dir_all(path)
                .with_context(|| format!("failed to create directory {path:?}"))?;
        }

        let config = serde_json::to_string(&config)?;
        fs::write(&self.path, config)
            .with_context(|| format!("failed to write file {:?}", self.path))
    }

    pub fn remove(&self) -> Result<()> {
        if util::exists(&self.path)? {
            fs::remove_file(&self.path)
                .with_context(|| format!("failed to remove {:?}", self.path))?;
        }

        Ok(())
    }
}
