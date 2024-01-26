//! Perform a version lookup.
use std::process::Stdio;

use tokio::process::Command;

use super::Result;

/// Exec a `kubectl` command to pull down the kubernetes version
/// metadata for a running cluster for use in the test framework
pub async fn get(kubectl_command: &str) -> Result<K8sVersion> {
    let mut command = Command::new(kubectl_command);
    command
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped());

    command.arg("version");
    command.arg("-o").arg("json");

    command.kill_on_drop(true);

    let reader = command.output().await?;
    let json: serde_json::Value = serde_json::from_slice(&reader.stdout)?;

    Ok(K8sVersion {
        major: json["serverVersion"]["major"].to_string().replace('\"', ""),
        minor: json["serverVersion"]["minor"].to_string().replace('\"', ""),
        platform: json["serverVersion"]["platform"]
            .to_string()
            .replace('\"', ""),
        git_version: json["serverVersion"]["gitVersion"]
            .to_string()
            .replace('\"', ""),
    })
}

/// Maps K8s version metadata to struct to provide accessor
/// methods for use in testing framework
#[derive(Debug)]
pub struct K8sVersion {
    /// Server Major Version
    major: String,
    /// Server Minor Version
    minor: String,
    /// Server Platform Target
    platform: String,
    /// Fully Qualified Version Number
    git_version: String,
}

impl K8sVersion {
    /// Accessor method for returning major version
    pub fn major(&self) -> String {
        self.major.to_string()
    }

    /// Accessor method for returning minor version
    pub fn minor(&self) -> String {
        self.minor.to_string()
    }

    /// Accessor method for returning platform target
    pub fn platform(&self) -> String {
        self.platform.to_string()
    }

    /// Accessor method for returning fully qualified version
    pub fn version(&self) -> String {
        self.git_version.to_string()
    }
}
