use super::Transform;
use crate::{
    dns::Resolver,
    event::{Event, ValueKind},
    runtime::TaskExecutor,
    sinks::util::{
        http::https_client,
        tls::{TlsOptions, TlsSettings},
    },
    sources::kubernetes::POD_UID,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use bytes::Bytes;
use futures::stream::Stream;
use futures03::compat::Future01CompatExt;
use http::{
    header,
    uri::{self, Scheme},
    Request, Uri,
};
use hyper::client::HttpConnector;
use hyper::Body;
use hyper_tls::HttpsConnector;
use k8s_openapi::{
    self as k8s,
    api::core::v1 as api,
    api::core::v1::{Pod, WatchPodForAllNamespacesResponse},
    apimachinery::pkg::apis::meta::v1::WatchEvent,
    Response, ResponseError,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, RwLock};
use string_cache::DefaultAtom as Atom;

// ************************ Defined by Kubernetes *********************** //
// API access is mostly defined with
// https://kubernetes.io/docs/tasks/access-application-cluster/access-cluster/#accessing-the-api-from-a-pod
//
// And Kubernetes service data with
// https://kubernetes.io/docs/concepts/containers/container-environment-variables/#cluster-information

/// File in which Kubernetes stores service account token.
const TOKEN_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";

/// Enviroment variable which contains host to Kubernetes API.
const HOST_ENV: &str = "KUBERNETES_SERVICE_HOST";

/// Enviroment variable which contains port to Kubernetes API.
const PORT_ENV: &str = "KUBERNETES_SERVICE_PORT";

/// Path to certificate authority certificate
const CA_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";

// *********************** Defined by Vector **************************** //
/// Node name `spec.nodeName` of Vector pod passed down with Downward API.
const NODE_NAME_ENV: &str = "VECTOR_NODE_NAME";

/// Prefiks for all metadata fields
const FIELD_PREFIX: &str = "pod.";

// type Pod = kube::api::Object<PodSpec, k8s_openapi::api::core::v1::PodStatus>;

/// Shared HashMap of (key,value) fields for pods on this node.
/// Joined on key - pod_uid field.
///
/// Mutex should work fine for this case.
type JoinMap = Arc<RwLock<HashMap<Bytes, Vec<(Atom, ValueKind)>>>>;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct KubePodMetadata {
    #[serde(default = "default_fields")]
    fields: Vec<String>,
}

inventory::submit! {
    TransformDescription::new_without_default::<KubePodMetadata>("kubernetes_pod_metadata")
}

#[typetag::serde(name = "kubernetes_pod_metadata")]
impl TransformConfig for KubePodMetadata {
    fn build(&self, exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        // Main idea is to have a background task which will premptively
        // acquire metadata for all pods on this node, and then maintaine that.

        // TODO: use real Resolver
        let client = MetadataClient::new(
            self,
            Resolver::new(vec![], exec.clone()).unwrap(),
            node_name()?,
            account_token(),
            kubernetes_host()?,
            kubernetes_port()?,
            tls_settings()?,
        )?;
        // Dry run
        client.watch_pods_request(None)?;

        let transform = KubernetesPodMetadata {
            metadata: client.metadata(),
        };

        exec.spawn_std(async move {
            match client.run().await {
                Ok(_) => unreachable!(),
                Err(error) => error!(
                    message = "Kubernetes background metadata client stoped.",
                    cause = ?error
                ),
            }
        });

        Ok(Box::new(transform))
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
    std::env::var(NODE_NAME_ENV).map_err(|_| BuildError::MissingNodeName { env: NODE_NAME_ENV })
}

fn kubernetes_host() -> Result<String, BuildError> {
    std::env::var(HOST_ENV).map_err(|_| BuildError::NoKubernetes {
        reason: format!("Missing Kubernetes API host defined with {}", HOST_ENV),
    })
}

fn kubernetes_port() -> Result<String, BuildError> {
    std::env::var(PORT_ENV).map_err(|_| BuildError::NoKubernetes {
        reason: format!("Missing Kubernetes API port defined with {}", PORT_ENV),
    })
}

fn account_token() -> Option<String> {
    fs::read(TOKEN_PATH)
        .map_err(|error| {
            warn!(
                message = "Missing Kubernetes service account token file.",
                ?error
            )
        })
        .ok()
        .and_then(|bytes| {
            String::from_utf8(bytes)
                .map_err(|error| {
                    warn!(
                        message = "Kubernetes service account token file is not a valid utf8.",
                        ?error
                    )
                })
                .ok()
        })
}

fn tls_settings() -> Result<TlsSettings, BuildError> {
    let mut options = TlsOptions::default();
    options.ca_path = Some(CA_PATH.into());
    TlsSettings::from_options(&Some(options)).context(TlsError)
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("{}, probably because Vector isn't in Kubernetes Pod.", reason))]
    NoKubernetes { reason: String },
    #[snafu(display("TLS construction errored {}", source))]
    TlsError { source: crate::Error },
    #[snafu(display("Http client construction errored {}", source))]
    HttpError { source: crate::Error },
    #[snafu(display(
        "Missing environment variable {:?} containing node name `spec.nodeName`.",
        env
    ))]
    MissingNodeName { env: &'static str },
    #[snafu(display("Failed constructing request: {}", source))]
    K8SOpenapiError { source: k8s::RequestError },
    #[snafu(display("Uri gotten from Kubernetes is invalid: {}", source))]
    InvalidUri { source: uri::InvalidUri },
    #[snafu(display("Uri gotten from Kubernetes is invalid: {}", source))]
    InvalidUriParts { source: uri::InvalidUriParts },
    #[snafu(display("Authorization token gotten from Kubernetes is invalid: {}", source))]
    InvalidToken { source: header::InvalidHeaderValue },
}

struct MetadataClient {
    fields: Vec<Box<dyn Fn(&Pod) -> Vec<(Atom, ValueKind)> + Send + Sync + 'static>>,
    metadata: JoinMap,
    node_name: String,
    token: Option<String>,
    host: String,
    port: String,
    client: hyper::Client<HttpsConnector<HttpConnector<Resolver>>>,
}

impl MetadataClient {
    fn new(
        trans_config: &KubePodMetadata,
        resolver: Resolver,
        node_name: String,
        token: Option<String>,
        host: String,
        port: String,
        tls_settings: TlsSettings,
    ) -> Result<Self, BuildError> {
        Ok(Self {
            fields: all_fields()
                .into_iter()
                .filter(|(key, _)| {
                    trans_config
                        .fields
                        .iter()
                        .any(|field| field.as_str() == *key)
                })
                .map(|(_, fun)| fun)
                .collect(),
            metadata: Arc::default(),
            node_name,
            token,
            host,
            port,
            client: https_client(resolver, tls_settings).context(HttpError)?,
        })
    }

    fn metadata(&self) -> JoinMap {
        self.metadata.clone()
    }

    /// Errors only if it would always error
    async fn run(&self) -> Result<(), BuildError> {
        // If watch is initiated with None resource_version, we will receive initial
        // list of pods as synthetic "Added" events.
        // https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
        let mut version = None;

        // Restarts watch request
        loop {
            // We could clear Metadata map at this point, as Kubernets documentation suggests,
            // but then we would have a time gap during which events wouldn't be enriched
            // with metadata.
            version = self
                .request(
                    version.clone(),
                    self.watch_pods_request(version)?,
                    |response| self.watch_process(response),
                )
                .await;
        }
    }

    /// Resolves request to multiple R data.
    async fn request<R: Response>(
        &self,
        mut version: Option<Version>,
        request: Request<Body>,
        process: impl Fn(R) -> VersionResult,
    ) -> Option<Version> {
        let response = self.client.request(request).compat().await;
        match response {
            Ok(response) => {
                info!(message = "Watching Pod list for changes.");
                let status = response.status();
                let mut unused = Vec::new();
                let mut body = response.into_body();
                'watch: loop {
                    // We need to process Chunks as they come because watch behaves like
                    // a never ending stream of Chunks.
                    match body.into_future().compat().await {
                        Ok((chunk, tmp_body)) => {
                            body = tmp_body;

                            if let Some(chunk) = chunk {
                                unused.extend_from_slice(chunk.as_ref());

                                // Parse then process, recieved unused data
                                'process: loop {
                                    match R::try_from_parts(status, &unused) {
                                        Ok((data, used_bytes)) => {
                                            match process(data) {
                                                VersionResult::New(new_version) => {
                                                    // Store last resourceVersion
                                                    // https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes
                                                    version = new_version.or(version);

                                                    assert!(
                                                        used_bytes > 0,
                                                        "Parser must consume some data"
                                                    );

                                                    let _ = unused.drain(..used_bytes);
                                                    continue 'process;
                                                }
                                                VersionResult::Reload => (),
                                                VersionResult::Restart => return None,
                                            }
                                        }
                                        Err(ResponseError::NeedMoreData) => continue 'watch,
                                        Err(error) => debug!(
                                            "Unable to parse {:?} from response. Error: {:?}",
                                            std::any::type_name::<R>(),
                                            error
                                        ),
                                    }
                                    break 'watch;
                                }
                            }
                        }
                        Err(error) => debug!(message = "Watch request failed.", ?error),
                    }
                    break 'watch;
                }
            }
            Err(error) => debug!(message = "Failed resolving request.", ?error),
        }

        version
    }

    /// Err when metadata should be refetched.
    fn watch_process(&self, response: WatchPodForAllNamespacesResponse) -> VersionResult {
        match response {
            WatchPodForAllNamespacesResponse::Ok(event) => {
                match event {
                    WatchEvent::Added(pod)
                    | WatchEvent::Modified(pod)
                    | WatchEvent::Bookmark(pod)
                    | WatchEvent::Deleted(pod) => {
                        // In the case of Deleted, we don't delete it's data, as there could still exist unprocessed logs from that pod.
                        // Not deleteing will cause "memory leakage" in a sense that the data won't be used ever
                        // again after some point, but the catch is that we don't know when that point is.

                        let _ = self.update(&pod);

                        VersionResult::New(
                            pod.metadata.as_ref().and_then(|metadata| {
                                metadata.resource_version.clone().map(Version)
                            }),
                        )
                    }
                    WatchEvent::ErrorStatus(status) => {
                        // 410 Gone, restart with new list.
                        if status.code == Some(410) {
                            warn!(message = "Pod list desynced. Reseting list.", cause = ?status);
                            VersionResult::Restart
                        } else {
                            debug!("Watch event with error status: {:?}", status);
                            VersionResult::New(None)
                        }
                    }
                    WatchEvent::ErrorOther(value) => {
                        debug!(?value);
                        VersionResult::New(None)
                    }
                }
            }
            WatchPodForAllNamespacesResponse::Other(Ok(_)) => {
                debug!(message = "Received wrong object from Kubernetes API.");
                VersionResult::New(None)
            }
            WatchPodForAllNamespacesResponse::Other(Err(error)) => {
                debug!(message = "Failed parsing watch list of Pods.", ?error);
                VersionResult::Reload
            }
        }
    }

    fn watch_pods_request(
        &self,
        resource_version: Option<Version>,
    ) -> Result<Request<Body>, BuildError> {
        // Prepare request
        let (mut request, _) = api::Pod::watch_pod_for_all_namespaces(k8s::WatchOptional {
            field_selector: Some(self.field_selector().as_str()),
            resource_version: resource_version.as_ref().map(|v| v.0.as_str()),
            ..Default::default()
        })
        .context(K8SOpenapiError)?;

        self.authorize(&mut request)?;
        self.fill_uri(&mut request)?;

        let (parts, body) = request.into_parts();
        Ok(Request::from_parts(parts, body.into()))
    }

    fn authorize(&self, request: &mut Request<Vec<u8>>) -> Result<(), BuildError> {
        if let Some(token) = self.token.as_ref() {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(format!("Bearer {}", token).as_str())
                    .context(InvalidToken)?,
            );
        }

        Ok(())
    }

    fn fill_uri(&self, request: &mut Request<Vec<u8>>) -> Result<(), BuildError> {
        let mut uri = request.uri().clone().into_parts();
        uri.scheme = Some(Scheme::HTTPS);
        uri.authority = Some(
            format!("{}:{}", self.host, self.port)
                .parse()
                .context(InvalidUri)?,
        );
        *request.uri_mut() = Uri::from_parts(uri).context(InvalidUriParts)?;

        Ok(())
    }

    fn field_selector(&self) -> String {
        format!("spec.nodeName={}", self.node_name)
    }

    fn update(&self, pod: &Pod) -> Option<()> {
        // trace!(message = "Trying to update Pod metadata.");
        let uid: Bytes = pod.metadata.as_ref()?.uid.as_ref()?.as_str().into();

        let fields = self.fields(pod);

        // TODO: This is blocking
        let mut map = self.metadata.write().ok()?;

        trace!(message = "Updated Pod metadata.", uid = ?uid);

        map.insert(uid, fields);
        Some(())
    }

    fn fields(&self, pod: &Pod) -> Vec<(Atom, ValueKind)> {
        self.fields.iter().flat_map(|fun| fun(pod)).collect()
    }
}

/// Version of Kubernetes resource
#[derive(Clone, Debug)]
struct Version(String);

#[derive(Clone, Debug)]
enum VersionResult {
    /// Potentialy newer version
    New(Option<Version>),
    /// Start new request with current version.
    Reload,
    /// Start new request with None version.
    Restart,
}

pub struct KubernetesPodMetadata {
    metadata: JoinMap,
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

fn default_fields() -> Vec<String> {
    vec!["name", "namespace", "labels", "annotations", "node_name"]
        .into_iter()
        .map(Into::into)
        .collect()
}

/// Returns list of all supported fields and their extraction function.
fn all_fields() -> Vec<(
    &'static str,
    Box<dyn Fn(&Pod) -> Vec<(Atom, ValueKind)> + Send + Sync + 'static>,
)> {
    vec![
        // ------------------------ ObjectMeta ------------------------ //
        field("name", |pod| pod.metadata.as_ref()?.name.clone()),
        field("namespace", |pod| pod.metadata.as_ref()?.namespace.clone()),
        field("creation_timestamp", |pod| {
            pod.metadata
                .as_ref()?
                .creation_timestamp
                .clone()
                .map(|time| time.0)
        }),
        field("deletion_timestamp", |pod| {
            pod.metadata
                .as_ref()?
                .deletion_timestamp
                .clone()
                .map(|time| time.0)
        }),
        collection_field("labels", |pod| pod.metadata.as_ref()?.labels.as_ref()),
        collection_field("annotations", |pod| {
            pod.metadata.as_ref()?.annotations.as_ref()
        }),
        // ------------------------ PodSpec ------------------------ //
        field("node_name", |pod| pod.spec.as_ref()?.node_name.clone()),
        field("hostname", |pod| pod.spec.as_ref()?.hostname.clone()),
        field("priority", |pod| pod.spec.as_ref()?.priority),
        field("priority_class_name", |pod| {
            pod.spec.as_ref()?.priority_class_name.clone()
        }),
        field("service_account_name", |pod| {
            pod.spec.as_ref()?.service_account_name.clone()
        }),
        field("subdomain", |pod| pod.spec.as_ref()?.subdomain.clone()),
        // ------------------------ PodStatus ------------------------ //
        field("host_ip", |pod| pod.status.as_ref()?.host_ip.clone()),
        field("ip", |pod| pod.status.as_ref()?.pod_ip.clone()),
    ]
}

fn field<T: Into<ValueKind>>(
    name: &'static str,
    fun: impl Fn(&Pod) -> Option<T> + Send + Sync + 'static,
) -> (
    &'static str,
    Box<dyn Fn(&Pod) -> Vec<(Atom, ValueKind)> + Send + Sync + 'static>,
) {
    let key: Atom = with_prefix(name).into();
    let fun = move |pod: &Pod| {
        fun(pod)
            .map(|data| vec![(key.clone(), data.into())])
            .unwrap_or_default()
    };
    (name, Box::new(fun) as Box<_>)
}

fn collection_field(
    name: &'static str,
    fun: impl Fn(&Pod) -> Option<&BTreeMap<String, String>> + Send + Sync + 'static,
) -> (
    &'static str,
    Box<dyn Fn(&Pod) -> Vec<(Atom, ValueKind)> + Send + Sync + 'static>,
) {
    let prefix_key = with_prefix(name) + ".";
    let fun = move |pod: &Pod| {
        fun(pod)
            .map(|map| {
                map.iter()
                    .map(|(key, value)| ((prefix_key.clone() + key).into(), value.into()))
                    .collect()
            })
            .unwrap_or_default()
    };
    (name, Box::new(fun) as Box<_>)
}

fn with_prefix(name: &str) -> String {
    FIELD_PREFIX.to_owned() + name
}
