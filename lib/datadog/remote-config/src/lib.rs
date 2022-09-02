use std::collections::{HashMap, HashSet};

use futures::AsyncReadExt;
use hyper::{client::HttpConnector, Body, Method};
use hyper_openssl::HttpsConnector;
use prost::Message;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use tuf::{
    crypto::HashAlgorithm,
    interchange,
    metadata::{Metadata, MetadataPath, MetadataVersion, RawSignedMetadata},
    repository::{EphemeralRepository, RepositoryProvider, RepositoryStorage},
};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.config.rs"));
}
mod error;

use error::Result;
use proto::{DelegatedMeta, LatestConfigsRequest, LatestConfigsResponse, TopMeta};

pub use error::Error;
pub use tuf::metadata::{TargetDescription, TargetPath};

type TUFClient = tuf::client::Client<
    interchange::Json,
    EphemeralRepository<interchange::Json>,
    EphemeralRepository<interchange::Json>,
>;

// TODO: add for environments other than staging
const CONFIG_ROOT: &[u8] = include_bytes!("../config_root.json");
const DIRECTOR_ROOT: &[u8] = include_bytes!("../director_root.json");

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
    products: HashSet<String>,
    new_products: HashSet<String>,
    active_client: proto::Client,
    backend_client_state: Vec<u8>,
    config_client: TUFClient,
    director_client: TUFClient,
}

impl Client {
    /// Initialize a new `Client` from the given configuration.
    ///
    /// This will make an initial empty request to the Remote Config service to bootstrap the client
    /// with some basic metadata, including which products are available.
    pub async fn initialize(config: Config) -> Result<Self> {
        let Config {
            site,
            api_key,
            app_key,
            hostname,
            agent_version,
        } = config;

        let conn = HttpsConnector::new().map_err(|e| Error::OpenSsl(e.to_string()))?;
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

        // The correct meta versions here will get filled in on `update`
        let client_state = proto::ClientState::default();

        let client_agent = proto::ClientAgent {
            name: String::from("vector"),
            version: agent_version.clone(),
        };

        let id = Alphanumeric.sample_string(&mut rand::thread_rng(), 21);

        let active_client = proto::Client {
            state: Some(client_state),
            id,
            is_agent: false,
            client_agent: Some(client_agent),
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .expect("unix epoch is in the past")
                .as_secs(),
            ..Default::default()
        };

        let local = EphemeralRepository::new();
        let remote = EphemeralRepository::new();
        let config = tuf::client::Config::default();
        let trusted_root = RawSignedMetadata::new(CONFIG_ROOT.to_vec());
        let config_client =
            tuf::client::Client::with_trusted_root(config, &trusted_root, local, remote).await?;

        let local = EphemeralRepository::new();
        let remote = EphemeralRepository::new();
        let config = tuf::client::Config::default();
        let trusted_root = RawSignedMetadata::new(DIRECTOR_ROOT.to_vec());
        let director_client =
            tuf::client::Client::with_trusted_root(config, &trusted_root, local, remote).await?;

        let mut client = Self {
            inner,
            hostname,
            agent_version,
            products: Default::default(),
            new_products: Default::default(),
            active_client,
            backend_client_state: Default::default(),
            config_client,
            director_client,
        };

        client.apply(response).await?;

        Ok(client)
    }

    /// Return a list of products available in the backend, based on the current metadata.
    pub fn available_products(&self) -> Result<Vec<String>> {
        Ok(self
            .config_client
            .database()
            .trusted_snapshot()
            .ok_or(Error::MissingSnapshotData)?
            .meta()
            .keys()
            .map(|key| key.to_string())
            .collect())
    }

    /// Add a new product for which we should receive targets on the next call to `update`. The
    /// valid possibilities can be found via `available_products`.
    pub fn add_product(&mut self, product: impl Into<String>) {
        self.new_products.insert(product.into());
    }

    /// Return the available target paths and their descriptions based on the current metadata.
    pub fn targets(&self) -> Result<&HashMap<TargetPath, TargetDescription>> {
        Ok(self
            .director_client
            .database()
            .trusted_targets()
            .ok_or(Error::MissingTargetData)?
            .targets())
    }

    /// Extract the custom version information about a particular target path. This is specific to
    /// our Remote Config implementation.
    pub fn target_version(&self, path: &TargetPath) -> Result<u64> {
        let desc = self
            .targets()?
            .get(path)
            .ok_or_else(|| Error::UnknownTarget(path.clone()))?;
        let custom = desc.custom().get("v").ok_or(Error::MissingTargetVersion)?;
        serde_json::from_value(custom.clone()).map_err(|_| Error::MissingTargetVersion)
    }

    /// Return the value of a particular target file as a `String`, checking both its length and
    /// hashes against the metadata in the config repo.
    pub async fn fetch_target(&self, path: &TargetPath) -> Result<String> {
        let desc = self
            .targets()?
            .get(path)
            .ok_or_else(|| Error::UnknownTarget(path.clone()))?;

        let expected_len = desc.length() as usize;
        let expected_hashes = tuf::crypto::retain_supported_hashes(desc.hashes());
        if expected_hashes.is_empty() {
            return Err(Error::NoSupportedHashes);
        }

        let path = path.with_hash_prefix(&expected_hashes[0].1)?;

        let mut read = self
            .director_client
            .remote_repo()
            .fetch_target(&path)
            .await?;
        let mut buf = Vec::new();
        read.read_to_end(&mut buf).await?;

        if buf.len() != expected_len {
            return Err(Error::BadLength);
        }

        let hash_algs = expected_hashes
            .iter()
            .map(|(alg, _val)| (*alg).clone())
            .collect::<Vec<HashAlgorithm>>();
        let actual_hashes = tuf::crypto::calculate_hashes_from_slice(&buf, hash_algs.as_slice())?;
        let expected = expected_hashes
            .into_iter()
            .map(|(alg, val)| (alg.clone(), val))
            .collect();

        if actual_hashes == expected {
            Ok(String::from_utf8_lossy(&buf).to_string())
        } else {
            Err(Error::BadHash)
        }
    }

    /// Make a request to the Remote Config service to receive any updated metadata and apply it to
    /// the client's current state.
    pub async fn update(&mut self) -> Result<()> {
        let current_config_snapshot_version = self
            .config_client
            .database()
            .trusted_snapshot()
            .ok_or(Error::MissingSnapshotData)?
            .version() as u64;
        let current_config_root_version =
            self.config_client.database().trusted_root().version() as u64;
        let current_director_root_version =
            self.director_client.database().trusted_root().version() as u64;
        let current_targets_version = self
            .config_client
            .database()
            .trusted_targets()
            .ok_or(Error::MissingTargetData)?
            .version() as u64;

        let mut request = LatestConfigsRequest {
            hostname: self.hostname.clone(),
            agent_version: self.agent_version.clone(),
            current_config_snapshot_version,
            current_config_root_version,
            current_director_root_version,
            products: self.products.clone().into_iter().collect(),
            new_products: self.new_products.clone().into_iter().collect(),
            backend_client_state: self.backend_client_state.clone(),
            ..Default::default()
        };

        let all_products = self.products.union(&self.new_products);
        self.active_client.products = all_products.cloned().collect();

        self.active_client.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .expect("unix epoch is in the past")
            .as_secs();

        let mut state = self
            .active_client
            .state
            .as_mut()
            .expect("always have client state");
        state.root_version = current_config_root_version;
        state.targets_version = current_targets_version;

        request.active_clients.push(self.active_client.clone());

        let response = self.inner.send_request(request).await?;

        self.apply(response).await?;

        self.products.extend(self.new_products.drain());

        Ok(())
    }

    async fn apply(&mut self, response: LatestConfigsResponse) -> Result<()> {
        // At a high level, what we're doing here is populating the "remote" repos with the metadata
        // that we received from upstream (which does not validate it), and then using the clients'
        // `update` methods to synchronize that metadata to the "local" repos, during which
        // validation is performed.

        for target_file in &response.target_files {
            self.director_client
                .remote_repo_mut()
                .store_target(
                    &TargetPath::new(&target_file.path)?,
                    &mut target_file.raw.as_slice(),
                )
                .await?;
        }

        store(
            self.config_client.remote_repo_mut(),
            &MetadataPath::root(),
            &response
                .config_metas
                .as_ref()
                .ok_or(Error::MissingConfigMetas)?
                .roots,
        )
        .await?;

        store(
            self.config_client.remote_repo_mut(),
            &MetadataPath::timestamp(),
            &response
                .config_metas
                .as_ref()
                .ok_or(Error::MissingConfigMetas)?
                .timestamp,
        )
        .await?;

        store(
            self.config_client.remote_repo_mut(),
            &MetadataPath::snapshot(),
            &response
                .config_metas
                .as_ref()
                .ok_or(Error::MissingConfigMetas)?
                .snapshot,
        )
        .await?;

        store(
            self.config_client.remote_repo_mut(),
            &MetadataPath::targets(),
            &response
                .config_metas
                .as_ref()
                .ok_or(Error::MissingConfigMetas)?
                .top_targets,
        )
        .await?;

        store(
            self.config_client.remote_repo_mut(),
            &MetadataPath::targets(),
            &response
                .config_metas
                .as_ref()
                .ok_or(Error::MissingConfigMetas)?
                .delegated_targets,
        )
        .await?;

        store(
            self.director_client.remote_repo_mut(),
            &MetadataPath::root(),
            &response
                .director_metas
                .as_ref()
                .ok_or(Error::MissingDirectorMetas)?
                .roots,
        )
        .await?;

        store(
            self.director_client.remote_repo_mut(),
            &MetadataPath::timestamp(),
            &response
                .director_metas
                .as_ref()
                .ok_or(Error::MissingDirectorMetas)?
                .timestamp,
        )
        .await?;

        store(
            self.director_client.remote_repo_mut(),
            &MetadataPath::snapshot(),
            &response
                .director_metas
                .as_ref()
                .ok_or(Error::MissingDirectorMetas)?
                .snapshot,
        )
        .await?;

        store(
            self.director_client.remote_repo_mut(),
            &MetadataPath::targets(),
            &response
                .director_metas
                .as_ref()
                .ok_or(Error::MissingDirectorMetas)?
                .targets,
        )
        .await?;

        self.config_client.update().await?;
        self.director_client.update().await?;

        // The Remote Config service uses a `custom` field at the top-level of the targets metadata
        // to store this field which we are supposed to echo back to the server. That `custom` field
        // is not explicitly part of the TUF spec, which is why we need to pull it out of the
        // `additional_fields` catch-all here.
        if let Some(state) = self
            .director_client
            .database()
            .trusted_targets()
            .map(|t| t.additional_fields())
            .and_then(|t| t.get("custom"))
            .and_then(|t| t.get("opaque_backend_state"))
            .and_then(|t| t.as_str())
            .and_then(|t| base64::decode(t).ok())
        {
            self.backend_client_state = state;
        }

        Ok(())
    }
}

async fn store<'a, T, U>(
    repo: &mut dyn RepositoryStorage<interchange::Json>,
    path: &MetadataPath,
    tms: T,
) -> Result<()>
where
    T: IntoIterator<Item = &'a U> + 'a,
    U: StorableMeta + 'a,
{
    let mut latest_version = 0;
    for tm in tms {
        repo.store_metadata(
            path,
            MetadataVersion::Number(tm.version() as _),
            &mut tm.raw(),
        )
        .await?;

        if tm.version() >= latest_version {
            latest_version = tm.version();
            repo.store_metadata(path, MetadataVersion::None, &mut tm.raw())
                .await?;
        }
    }
    Ok(())
}

trait StorableMeta {
    fn version(&self) -> u64;
    fn raw(&self) -> &[u8];
}

impl StorableMeta for TopMeta {
    fn version(&self) -> u64 {
        self.version
    }

    fn raw(&self) -> &[u8] {
        self.raw.as_slice()
    }
}

impl StorableMeta for DelegatedMeta {
    fn version(&self) -> u64 {
        self.version
    }

    fn raw(&self) -> &[u8] {
        self.raw.as_slice()
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

        let response = self.http.request(request).await?;

        let status = response.status();
        let body = hyper::body::to_bytes(response.into_body()).await?;

        if status.is_success() {
            Ok(LatestConfigsResponse::decode(body)?)
        } else {
            let body = String::from_utf8_lossy(&body).to_string();
            Err(Error::HttpUnexpectedStatus { status, body })
        }
    }
}
