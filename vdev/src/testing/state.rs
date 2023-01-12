use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::platform;

const CONFIG_FILE: &str = "config.json";

pub fn envs_dir(integration: &str) -> PathBuf {
    [
        &platform::data_dir(),
        Path::new("integration/envs"),
        Path::new(integration),
    ]
    .iter()
    .collect()
}

pub fn env_exists(envs_dir: &Path, environment: &str) -> bool {
    let dir: PathBuf = [envs_dir, Path::new(environment)].iter().collect();
    dir.is_dir()
}

pub fn active_envs(envs_dir: &Path) -> Result<HashSet<String>> {
    let mut environments = HashSet::new();
    if !envs_dir.is_dir() {
        return Ok(environments);
    }

    for entry in envs_dir
        .read_dir()
        .with_context(|| format!("failed to read directory {}", envs_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read directory entry {}", envs_dir.display()))?;
        if entry.path().is_dir() {
            environments.insert(entry.file_name().into_string().unwrap());
        }
    }

    Ok(environments)
}

pub fn save_env(envs_dir: &Path, environment: &str, config: &str) -> Result<()> {
    let mut path = envs_dir.join(environment);
    if !path.is_dir() {
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create directory {}", path.display()))?;
    }

    path.push(CONFIG_FILE);
    fs::write(&path, config).with_context(|| format!("failed to write file {}", path.display()))?;

    Ok(())
}

pub fn read_env_config(envs_dir: &Path, environment: &str) -> Result<String> {
    let mut config_file = envs_dir.join(environment);
    config_file.push(CONFIG_FILE);

    fs::read_to_string(&config_file)
        .with_context(|| format!("failed to write file {}", config_file.display()))
}

pub fn remove_env(envs_dir: &Path, environment: &str) -> Result<()> {
    let env_path = envs_dir.join(environment);
    if env_path.is_dir() {
        fs::remove_dir_all(&env_path)
            .with_context(|| format!("failed to remove directory {env_path:?}"))?;
    }

    Ok(())
}
