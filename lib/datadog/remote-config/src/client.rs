use crate::{
    metas::{ConfigMeta, ConfigMetas, DelegatedTargets, DirectorMetas, Role},
    proto::{self, LatestConfigsRequest, LatestConfigsResponse},
    Version,
};
use anyhow::{bail, Result};
use hyper::{client::HttpConnector, Body, Method};
use hyper_openssl::HttpsConnector;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub site: String,
    pub api_key: String,
    pub app_key: String,
    pub hostname: String,
    pub agent_version: String,
}

#[derive(Debug)]
struct Inner {
    http: hyper::Client<HttpsConnector<HttpConnector>>,
    site: String,
    api_key: String,
    app_key: String,
}

pub struct Client {
    inner: Inner,
    hostname: String,
    agent_version: String,
    versions: TUFVersions,
    products: HashSet<String>,
    new_products: HashSet<String>,
    active_client: proto::Client,
    backend_client_state: Vec<u8>,
    pub available_products: Vec<String>,
    pub target_files: HashMap<String, Vec<u8>>,
    pub config_metas: ConfigMetas,
    pub director_metas: DirectorMetas,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RcClient")
            .field("versions", &self.versions)
            .field("products", &self.products)
            .field("new_products", &self.new_products)
            .field("available_products", &self.available_products)
            .field("config_metas", &self.config_metas)
            .field("director_metas", &self.director_metas)
            .field("target_files", &TargetFilesDebug(&self.target_files))
            .finish_non_exhaustive()
    }
}

struct TargetFilesDebug<'a>(&'a HashMap<String, Vec<u8>>);

impl fmt::Debug for TargetFilesDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for (k, v) in self.0 {
            map.key(&k);
            map.value(&String::from_utf8_lossy(&v));
        }
        map.finish()
    }
}

#[derive(Debug)]
struct TUFVersions {
    director_root: Version,
    director_targets: Version,
    config_root: Version,
    config_snapshot: Version,
}

impl Client {
    pub async fn initialize(config: Config) -> Result<Self> {
        let Config {
            site,
            api_key,
            app_key,
            hostname,
            agent_version,
        } = config;

        let conn = HttpsConnector::new().unwrap();
        let inner = Inner {
            http: hyper::Client::builder().build(conn),
            site,
            api_key,
            app_key,
        };

        let request = LatestConfigsRequest {
            hostname: hostname.clone(),
            agent_version: agent_version.clone(),
            ..Default::default()
        };

        // Send an initial request to fill out the rest of the metadata
        let response = inner.send_request(request).await?;

        // TODO: Starting with the configured root metas, verify the chain of roots up until the
        // latest returned from the above initial request.

        let versions = TUFVersions::from(&response);

        let client_state = proto::ClientState {
            // TODO: is this the right root?
            root_version: versions.config_root,
            targets_version: versions.director_targets,
            ..Default::default()
        };

        // TODO: don't lie
        let client_agent = proto::ClientAgent {
            name: String::from("trace-agent"),
            version: String::from("7.38.0-devel+git.58.9cc8e5c"),
        };

        let active_client = proto::Client {
            state: Some(client_state),
            // TODO: new randon string each time?
            id: String::from("gkyq_fM8iLbkG7MEZvGxW"),
            // TODO: are we?
            is_agent: true,
            client_agent: Some(client_agent),
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ..Default::default()
        };

        let mut client = Self {
            inner,
            hostname,
            agent_version,
            versions,
            products: Default::default(),
            new_products: Default::default(),
            active_client,
            backend_client_state: Default::default(),
            available_products: Default::default(),
            target_files: Default::default(),
            config_metas: Default::default(),
            director_metas: Default::default(),
        };

        client.apply(response);

        Ok(client)
    }

    pub fn add_product(&mut self, product: impl Into<String>) {
        self.new_products.insert(product.into());
    }

    pub async fn update(&mut self) -> Result<()> {
        let mut request = LatestConfigsRequest {
            hostname: self.hostname.clone(),
            agent_version: self.agent_version.clone(),
            current_config_snapshot_version: self.versions.config_snapshot,
            current_config_root_version: self.versions.config_root,
            current_director_root_version: self.versions.director_root,
            products: self.products.clone().into_iter().collect(),
            new_products: self.new_products.clone().into_iter().collect(),
            backend_client_state: self.backend_client_state.clone(),
            ..Default::default()
        };

        let all_products = self.products.union(&self.new_products);
        self.active_client.products = all_products.cloned().collect();

        self.active_client.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        request.active_clients.push(self.active_client.clone());

        // TODO: do this properly?
        // request.backend_client_state = r#"{"file_hashes":[]}"#.into();

        let response = self.inner.send_request(request).await?;

        self.apply(response);

        self.products.extend(self.new_products.drain());

        Ok(())
    }

    fn apply(&mut self, response: LatestConfigsResponse) {
        // Store target files
        for target_file in &response.target_files {
            self.target_files
                .insert(target_file.path.clone(), target_file.raw.clone());
        }

        // Populate config metas
        for root in &response.config_metas.as_ref().unwrap().roots {
            let parsed: ConfigMeta = serde_json::from_slice(&root.raw).unwrap();
            if let Role::Root(inner) = parsed.signed {
                self.config_metas.root.insert(root.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        if let Some(tm) = &response.config_metas.as_ref().unwrap().timestamp {
            let parsed: ConfigMeta = serde_json::from_slice(&tm.raw).unwrap();
            if let Role::Timestamp(inner) = parsed.signed {
                self.config_metas.timestamp.insert(tm.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        if let Some(tm) = &response.config_metas.as_ref().unwrap().snapshot {
            let parsed: ConfigMeta = serde_json::from_slice(&tm.raw).unwrap();
            if let Role::Snapshot(inner) = parsed.signed {
                self.available_products = inner
                    .meta
                    .keys()
                    .map(|key| key.rsplit_once('.').unwrap().0.to_string())
                    .collect();
                self.config_metas.snapshot.insert(tm.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        if let Some(tm) = &response.config_metas.as_ref().unwrap().top_targets {
            let parsed: ConfigMeta = serde_json::from_slice(&tm.raw).unwrap();
            if let Role::Targets(inner) = parsed.signed {
                self.config_metas.top_targets.insert(tm.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        for target in &response.config_metas.as_ref().unwrap().delegated_targets {
            let parsed: ConfigMeta = serde_json::from_slice(&target.raw).unwrap();
            if let Role::Targets(targets) = parsed.signed {
                self.config_metas.delegated_targets.push(DelegatedTargets {
                    version: target.version,
                    role: target.role.clone(),
                    targets,
                });
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }

        // Populate director metas
        for root in &response.director_metas.as_ref().unwrap().roots {
            let parsed: ConfigMeta = serde_json::from_slice(&root.raw).unwrap();
            if let Role::Root(inner) = parsed.signed {
                self.director_metas.root.insert(root.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        if let Some(tm) = &response.director_metas.as_ref().unwrap().timestamp {
            let parsed: ConfigMeta = serde_json::from_slice(&tm.raw).unwrap();
            if let Role::Timestamp(inner) = parsed.signed {
                self.director_metas.timestamp.insert(tm.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        if let Some(tm) = &response.director_metas.as_ref().unwrap().snapshot {
            let parsed: ConfigMeta = serde_json::from_slice(&tm.raw).unwrap();
            if let Role::Snapshot(inner) = parsed.signed {
                self.director_metas.snapshot.insert(tm.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
        if let Some(tm) = &response.director_metas.as_ref().unwrap().targets {
            let parsed: ConfigMeta = serde_json::from_slice(&tm.raw).unwrap();
            if let Role::Targets(inner) = parsed.signed {
                self.director_metas.targets.insert(tm.version, inner);
            } else {
                panic!("unexpected meta type: {:?}", parsed);
            }
        }
    }
}

impl From<&LatestConfigsResponse> for TUFVersions {
    fn from(response: &LatestConfigsResponse) -> TUFVersions {
        let config_snapshot = response
            .config_metas
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .version;
        let config_root = response
            .config_metas
            .as_ref()
            .unwrap()
            .roots
            .iter()
            .map(|root| root.version)
            .max()
            .unwrap();
        let director_root = response
            .director_metas
            .as_ref()
            .unwrap()
            .roots
            .iter()
            .map(|root| root.version)
            .max()
            .unwrap();
        let director_targets = response
            .director_metas
            .as_ref()
            .unwrap()
            .targets
            .as_ref()
            .unwrap()
            .version;

        TUFVersions {
            director_root,
            director_targets,
            config_root,
            config_snapshot,
        }
    }
}

impl Inner {
    async fn send_request(&self, request: LatestConfigsRequest) -> Result<LatestConfigsResponse> {
        let body = request.encode_to_vec();

        let request = hyper::Request::builder()
            .method(Method::GET)
            .uri(format!(
                "https://config.{}/api/v0.1/configurations",
                self.site
            ))
            .header("Content-Type", "application/x-protobuf")
            .header("DD-Api-Key", &self.api_key)
            .header("DD-Application-Key", &self.app_key)
            .header("Accept-Encoding", "gzip")
            .body(Body::from(body))?;

        let response = self
            .http
            .request(request)
            .await
            .expect("failed to read HTTP response");

        let status = response.status();
        let headers = response.headers().clone();
        let body = hyper::body::to_bytes(response.into_body()).await?;

        if status.is_success() {
            Ok(LatestConfigsResponse::decode(body).unwrap())
        } else {
            let body = String::from_utf8_lossy(&body);
            bail!(
                "HTTP request did not succeed:

    status: {status:?}
    headers: {headers:?}
    body: {body}"
            )
        }
    }
}
