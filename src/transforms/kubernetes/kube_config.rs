#![cfg(feature = "kubernetes-integration-tests")]

use dirs;
use serde::{Deserialize, Serialize};
use serde_yaml;
use snafu::{ResultExt, Snafu};
use std::{fs::File, path::PathBuf};

/// Enviorment variable that can containa path to kubernetes config file.
const CONFIG_PATH: &str = "KUBECONFIG";

/// Loads configuration from local kubeconfig file, the same
/// one that kubectl uses.
/// None if such file doesn't exist.
pub fn load_kube_config() -> Option<Result<Config, KubeConfigLoadError>> {
    let path = std::env::var(CONFIG_PATH)
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".kube").join("config")))?;

    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            return Some(Err(KubeConfigLoadError::FileError { source: error }));
        }
    };

    Some(serde_yaml::from_reader(file).context(ParsingError))
}

#[derive(Debug, Snafu)]
pub enum KubeConfigLoadError {
    #[snafu(display("Error opening Kubernetes config file: {}.", source))]
    FileError { source: std::io::Error },
    #[snafu(display("Error parsing Kubernetes config file: {}.", source))]
    ParsingError { source: serde_yaml::Error },
}

/// Config defines currently relevant data that can be found in
/// kubernetes config file, the same one that kubectl uses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub clusters: Vec<NamedCluster>,
    pub users: Vec<NamedUserInfo>,
    pub contexts: Vec<NamedContext>,
    #[serde(rename = "current-context")]
    pub current_context: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedCluster {
    pub name: String,
    pub cluster: Cluster,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cluster {
    pub server: String,
    #[serde(rename = "insecure-skip-tls-verify")]
    pub insecure_skip_tls_verify: Option<bool>,
    #[serde(rename = "certificate-authority")]
    pub certificate_authority: Option<String>,
    #[serde(rename = "certificate-authority-data")]
    pub certificate_authority_data: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedUserInfo {
    pub name: String,
    pub user: UserInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: Option<String>,
    pub password: Option<String>,

    pub token: Option<String>,
    #[serde(rename = "tokenFile")]
    pub token_file: Option<String>,

    #[serde(rename = "client-certificate")]
    pub client_certificate: Option<String>,
    #[serde(rename = "client-certificate-data")]
    pub client_certificate_data: Option<String>,

    #[serde(rename = "client-key")]
    pub client_key: Option<String>,
    #[serde(rename = "client-key-data")]
    pub client_key_data: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedContext {
    pub name: String,
    pub context: Context,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Context {
    pub cluster: String,
    pub user: String,
}
