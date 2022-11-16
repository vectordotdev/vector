use confy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ConfigFile {}

impl ConfigFile {
    pub fn new() -> Self {
        Self {}
    }

    pub fn path(&self) -> PathBuf {
        confy::get_configuration_file_path(self.app_name(), self.config_name())
            .expect("unable to find the config file")
    }

    pub fn load(&self) -> Config {
        confy::load(self.app_name(), self.config_name()).expect("unable to load config")
    }

    pub fn save(&self, config: Config) {
        confy::store(self.app_name(), self.config_name(), config).expect("unable to save config")
    }

    fn app_name(&self) -> &str {
        "vdev"
    }

    fn config_name(&self) -> &str {
        "config"
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub(crate) repo: String,
    pub(crate) org: String,
    pub(crate) orgs: BTreeMap<String, OrgConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OrgConfig {
    pub(crate) api_key: String,
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
            orgs: orgs,
        }
    }
}
