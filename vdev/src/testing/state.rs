use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE: &str = "config.json";

pub fn envs_dir(data_dir: &PathBuf, integration: &str) -> PathBuf {
    let mut envs_dir = data_dir.clone();
    envs_dir.extend(["integration", "envs", integration].iter());

    envs_dir
}

pub fn env_exists(envs_dir: &PathBuf, environment: &str) -> bool {
    envs_dir.join(environment).is_dir()
}

pub fn active_envs(envs_dir: &PathBuf) -> Result<HashSet<String>> {
    let mut environments = HashSet::new();
    if !envs_dir.is_dir() {
        return Ok(environments);
    }

    for entry in envs_dir
        .read_dir()
        .with_context(|| format!("failed to read directory {}", envs_dir.display()))?
    {
        if let Ok(entry) = entry {
            if entry.path().is_dir() {
                environments.insert(entry.file_name().into_string().unwrap());
            }
        }
    }

    Ok(environments)
}

pub fn save_env(envs_dir: &PathBuf, environment: &str, config: &String) -> Result<()> {
    let mut path = envs_dir.join(environment);
    if !path.is_dir() {
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create directory {}", path.display()))?;
    }

    path.push(CONFIG_FILE);
    fs::write(&path, config).with_context(|| format!("failed to write file {}", path.display()))?;

    Ok(())
}

pub fn read_env_config(envs_dir: &PathBuf, environment: &str) -> Result<String> {
    let mut config_file = envs_dir.join(environment);
    config_file.push(CONFIG_FILE);

    Ok(fs::read_to_string(&config_file)
        .with_context(|| format!("failed to write file {}", config_file.display()))?)
}

pub fn remove_env(envs_dir: &PathBuf, environment: &str) -> Result<()> {
    let env_dir = envs_dir.join(environment);
    if env_dir.is_dir() {
        fs::remove_dir_all(&env_dir)
            .with_context(|| format!("failed to remove directory {}", env_dir.display()))?;
    }

    Ok(())
}
