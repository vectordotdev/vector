use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const APP_NAME: &str = "vdev";
const FILE_STEM: &str = "config";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub repo: String,
    pub org: String,
    pub orgs: BTreeMap<String, OrgConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrgConfig {
    pub api_key: String,
}

impl Default for Config {
    fn default() -> Self {
        let default_org = "default".to_string();
        let mut orgs = BTreeMap::new();
        orgs.insert(
            default_org.clone(),
            OrgConfig {
                api_key: String::new(),
            },
        );

        Self {
            repo: "".to_string(),
            org: default_org,
            orgs,
        }
    }
}

pub fn path() -> Result<PathBuf> {
    confy::get_configuration_file_path(APP_NAME, FILE_STEM)
        .with_context(|| "unable to find the config file")
}

pub fn load() -> Result<Config> {
    confy::load(APP_NAME, FILE_STEM).with_context(|| "unable to load config")
}

pub fn save(config: Config) -> Result<()> {
    confy::store(APP_NAME, FILE_STEM, config).with_context(|| "unable to save config")
}
