use super::tls::{TlsOptions, TlsSettings};
use super::Transform;
use crate::{
    dns::DnsResolver,
    event::{Event, ValueKind},
    runtime::TaskExecutor,
    sinks::util::http::https_client,
    sources::kubernetes::POD_UID,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use futures::{stream, Stream};
use futures03::compat::Future01CompatExt;
use http::{header, uri::Scheme, Uri};
use k8s_openapi::{self as k8s, api::core::v1 as api};
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::timer::Delay;

// ************************ Defined by Kubernetes *********************** //
/// File in which Kubernetes stores service account token.
const TOKEN_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";

/// Enviroment variable which contains host to Kubernetes API.
const HOST_ENV: &str = "KUBERNETES_SERVICE_HOST";

/// Enviroment variable which contains port to Kubernetes API.
const PORT_ENV: &str = "KUBERNETES_SERVICE_PORT";

/// Path to certificate bundle
const CRT_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";

// *********************** Defined by Vector **************************** //
/// Node name `spec.nodeName` of Vector pod passed down with Downward API.
const NODE_NAME_ENV: &str = "VECTOR_NODE_NAME";

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct KubernetesPodMetadataConfig {
    fields: Vec<String>,
    namespace: Vec<String>,
}

inventory::submit! {
    TransformDescription::new_without_default::<KubernetesPodMetadataConfig>("kubernetes_pod_metadata")
}

#[typetag::serde(name = "kubernetes_pod_metadata")]
impl TransformConfig for AddFieldsConfig {
    fn build(&self, exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        // TODO: use main DnsResolver
        let client = MetadataClient::new(
            kubernetes_host()?,
            kubernetes_port()?,
            node_name()?,
            token(),
            tls_settings()?,
            DnsResolver::default(),
        );

        // dry run
        client.list_pods_request()?;

        // Main idea is to have a background task which will premptively
        // acquire metadata for all pods on this node, and then maintaine that.
        //
        // TODO: option
        // To avoid having a chance of blocking in Transform,
        // background task will channel changes to the transform.

        // exec.spawn(f: impl Future<Item = (), Error = ()> + Send + 'static);

        unimplemented!()
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "kubernetes_pod_metadata"
    }
}

fn node_name() -> Result<String, BuildError> {
    std::env::var(NODE_NAME_ENV).map_err(|_| MissingNodeName { env: NODE_NAME_ENV })
}

fn kubernetes_host() -> Result<String, BuildError> {
    std::env::var(HOST_ENV).map_err(|_| BuildError::NoKubernetes {
        source: "Missing Kubernetes API host",
    })
}

fn kubernetes_port() -> Result<String, BuildError> {
    std::env::var(PORT_ENV).map_err(|_| BuildError::NoKubernetes {
        source: "Missing Kubernetes API port",
    })
}

fn account_token() -> Option<String> {
    fs::read(TOKEN_PATH)
        .map_err(
            |error| warn!(message = "Missing Kubernetes service account token.",error = ?error),
        )
        .ok()
}

fn tls_settings() -> Result<TlsSettings, BuildError> {
    let mut options = TlsOptions::default();
    options.crt_path = Some(CRT_PATH.to_owned());
    TlsSettings::from_options(&options).context(TlsError)
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("{}, probably because Vector isn't in Kubernetes Pod.", source))]
    NoKubernetes { source: String },
    #[snafu(display("TLS construction errored {}", source))]
    TlsError { source: crate::Error },
    #[snafu(display(
        "Missing environment variable {:?} containing node name `spec.nodeName`.",
        env
    ))]
    MissingNodeName { env: &'static str },
    #[snafu(display("Failed constructing request: {}", source))]
    K8SOpenapiError { source: k8s::RequestError },
    #[snafu(display("Uri gotten from Kubernetes is invalid: {}", source))]
    InvalidUri { source: uri::InvalidUri },
    #[snafu(display("Authorization token gotten from Kubernetes is invalid: {}", source))]
    InvalidToken { source: header::InvalidHeaderValue },
}

struct MetadataClient {
    metadata: Arc<RwLock<HashMap<Atom, Vec<(Atom, ValueKind)>>>>,
    node_name: String,
    token: Option<String>,
    host: String,
    port: String,
    tls_settings: TlsSettings,
    resolver: DnsResolver,
}

impl MetadataClient {
    /// Stream of regets of list.
    fn new(
        host: String,
        port: String,
        node_name: String,
        token: Option<String>,
        tls_settings: TlsSettings,
        resolver: DnsResolver,
    ) -> Self {
        Ok(Self {
            metadata: Arc::default(),
            node_name,
            token,
            host,
            port,
            tls_settings,
        })
    }

    fn list_pods_request(&self) -> Result<Request<Vec<u8>>, BuildError> {
        // Prepare request
        let node_selector = format!("spec.nodeName={}", self.node_name);
        let (mut request, _) = api::Pod::list_pod_for_all_namespaces(k8s::ListOptional {
            field_selector: Some(node_selector.as_str()),
            ..Default::default()
        })
        .context(K8SOpenapiError)?;

        // Authorize
        if let Some(token) = self.token.as_ref() {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&format!("Bearer {}", token)?),
            );
        }

        // Fill Uri
        let mut uri = request.uri().clone().into_parts();
        uri.scheme = Some(Scheme::HTTPS);
        uri.authority = Some(
            format!("{}:{}", self.host, self.port)
                .parse()
                .context(InvalidUri)?,
        );
        *request.uri_mut() = uri;

        Ok(request)
    }

    /// Fails only if it would continue to fail indefinitely.
    async fn run(&self) -> Result<(), BuildError> {
        // Initialize metadata
        let list_version = self.fetch_pod_list().await?;

        // Watch
        unimplemented!();
    }

    /// Ok(list_version)
    async fn fetch_pod_list(&self) -> Result<String, BuildError> {
        loop {
            // Construct client
            let client = https_client(self.resolver.clone(), self.tls_settings.clone())?;

            match client.request(self.list_pods_request()?).await {
                Ok(response) => {
                    match <ListResponse<Pod> as Response>::try_from_parts(
                        response.status_code(),
                        response.body(),
                    ) {
                        Ok(pod_list) => {
                            for entry in pod_list
                                .items
                                .iter()
                                .filter_map(|pod| self.extract_fields(pod))
                            {
                                self.update(entry);
                            }

                            if let Some(metadata) = pod_list.metadata {
                                if let Some(version) = metadata.resource_version {
                                    return Ok(version);
                                }
                                debug!(message = "Missing pod list resource_version.")
                            } else {
                                debug!(message = "Missing pod list metadata.")
                            }
                        }
                        Other(Ok(_)) => {
                            debug!(message = "Received wrong object from Kubernetes API.")
                        }
                        Other(Err(error)) => {
                            debug!(message = "Failed parsing list of Pods.",error = ?error)
                        }
                    }
                }
                Err(error) => debug!(message = "Failed fetching list of Pods.",error = ?error),
            }

            warn!(message = "Retrying fetching list of Pods.");
            // Retry with delay
            Delay::new(Instant::now() + Duration::from_secs(1))
                .compat()
                .await
                .expect("Timer not set.");
        }
    }

    fn extract_fields(&self, pod: &Pod) -> Option<(Atom, Vec<(Atom, ValueKind)>)> {
        unimplemented!();
    }

    fn update(&self, entry: (Atom, Vec<(Atom, ValueKind)>)) {
        // Trace it
        unimplemented!();
    }
}

pub struct KubernetesPodMetadata {
    /// Shared HashMap of (key,value) fields for pods on this node.
    /// Joined on key - pod_uid field.
    ///
    /// Mutex should work fine for this case.
    metadata: Arc<RwLock<HashMap<Atom, Vec<(Atom, ValueKind)>>>>,
}

impl Transform for KubernetesPodMetadata {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();

        if let Some(ValueKind::Bytes(pod_uid)) = log.get(&POD_UID) {
            // TODO: This is blocking
            if let Some(metadata) = self.metadata.read().ok() {
                if let Some(fields) = metadata.get(pod_uid) {
                    for (key, value) in fields {
                        log.insert_implicit(key.clone(), value.clone());
                    }
                }
            }
        }

        Some(event)
    }
}
