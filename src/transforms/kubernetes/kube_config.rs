#![cfg(feature = "kubernetes-integration-tests")]

use dirs;
use serde_yaml;
use snafu::{ResultExt, Snafu};
use std::{fs::File, path::PathBuf};

pub use kube::config::Config;

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
