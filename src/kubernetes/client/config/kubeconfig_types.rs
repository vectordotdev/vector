// The contents of this module has been taken from https://github.com/clux/kube-rs
// without modification.
//
// # License: Apache License, Version 2.0
// # Copyright 2021 Datadog
//
// # Licensed under the Apache License, Version 2.0 (the "License");
// # you may not use this file except in compliance with the License.
// # You may obtain a copy of the License at
// #
// #      http://www.apache.org/licenses/LICENSE-2.0
// #
// # Unless required by applicable law or agreed to in writing, software
// # distributed under the License is distributed on an "AS IS" BASIS,
// # WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// # See the License for the specific language governing permissions and
// # limitations under the License.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// [`Kubeconfig`] represents information on how to connect to a remote Kubernetes cluster
/// that is normally stored in `~/.kube/config`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Kubeconfig {
    pub kind: Option<String>,
    #[serde(rename = "apiVersion")]
    pub api_version: Option<String>,
    pub preferences: Option<Preferences>,
    pub clusters: Vec<NamedCluster>,
    #[serde(rename = "users")]
    pub auth_infos: Vec<NamedAuthInfo>,
    pub contexts: Vec<NamedContext>,
    #[serde(rename = "current-context")]
    pub current_context: String,
    pub extensions: Option<Vec<NamedExtension>>,
}

/// Preferences stores extensions for cli.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preferences {
    pub colors: Option<bool>,
    pub extensions: Option<Vec<NamedExtension>>,
}

/// NamedExtension associates name with extension.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedExtension {
    pub name: String,
    pub extension: String,
}

/// NamedCluster associates name with cluster.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedCluster {
    pub name: String,
    pub cluster: Cluster,
}

/// Cluster stores information to connect Kubernetes cluster.
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

/// NamedAuthInfo associates name with authentication.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedAuthInfo {
    pub name: String,
    #[serde(rename = "user")]
    pub auth_info: AuthInfo,
}

/// AuthInfo stores information to tell cluster who you are.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthInfo {
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

    #[serde(rename = "as")]
    pub impersonate: Option<String>,
    #[serde(rename = "as-groups")]
    pub impersonate_groups: Option<Vec<String>>,

    #[serde(rename = "auth-provider")]
    pub auth_provider: Option<AuthProviderConfig>,

    pub exec: Option<ExecConfig>,
}

/// AuthProviderConfig stores auth for specified cloud provider.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthProviderConfig {
    pub name: String,
    pub config: HashMap<String, String>,
}

/// ExecConfig stores credential-plugin configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecConfig {
    #[serde(rename = "apiVersion")]
    pub api_version: Option<String>,
    pub args: Option<Vec<String>>,
    pub command: String,
    pub env: Option<Vec<HashMap<String, String>>>,
}

/// NamedContext associates name with context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedContext {
    pub name: String,
    pub context: Context,
}

/// Context stores tuple of cluster and user information.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Context {
    pub cluster: String,
    pub user: String,
    pub namespace: Option<String>,
    pub extensions: Option<Vec<NamedExtension>>,
}
