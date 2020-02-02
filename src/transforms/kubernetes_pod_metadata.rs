use super::Transform;
use crate::{
    dns::Resolver,
    event::{Event, LogEvent, Value},
    runtime::TaskExecutor,
    sinks::util::{
        http::https_client,
        tls::{TlsOptions, TlsSettings},
    },
    sources::kubernetes::OBJECT_UID,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use bytes::Bytes;
use evmap;
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
use std::fs;
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
const FIELD_PREFIX: &str = "pod_";

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
        //
        // Background task is writing to map of metadata from which Transform
        // is reading.

        let (map_read, map_write) = evmap::new();

        // TODO: use real Resolver
        let mut client = MetadataClient::new(
            self,
            map_write,
            Resolver::new(vec![], exec.clone()).unwrap(),
            node_name()?,
            account_token(),
            kubernetes_host()?,
            kubernetes_port()?,
            tls_settings()?,
        )?;
        // Dry request build
        client.watch_pods_request(None)?;

        exec.spawn_std(async move {
            let error = client.run().await;
            error!(
                message = "Kubernetes background metadata client stoped.",
                cause = ?error
            );
        });

        Ok(Box::new(KubernetesPodMetadata::new(map_read)))
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
    #[snafu(display("TLS construction errored {}.", source))]
    TlsError { source: crate::Error },
    #[snafu(display("Http client construction errored {}.", source))]
    HttpError { source: crate::Error },
    #[snafu(display(
        "Missing environment variable {:?} containing node name `spec.nodeName`.",
        env
    ))]
    MissingNodeName { env: &'static str },
    #[snafu(display("Failed constructing request: {}.", source))]
    K8SOpenapiError { source: k8s::RequestError },
    #[snafu(display("Uri gotten from Kubernetes is invalid: {}.", source))]
    InvalidUri { source: uri::InvalidUri },
    #[snafu(display("Uri gotten from Kubernetes is invalid: {}.", source))]
    InvalidUriParts { source: uri::InvalidUriParts },
    #[snafu(display("Authorization token gotten from Kubernetes is invalid: {}.", source))]
    InvalidToken { source: header::InvalidHeaderValue },
}

/// Background client which watches for Pod metadata changes and propagates them to Transform.
struct MetadataClient {
    fields: Vec<Box<dyn Fn(&Pod) -> Metadata + Send + Sync + 'static>>,
    /// Box<_> is needed because WriteHandle requiers Value to implement ShallowCopy which
    /// we won't implement ourselves as it require unsafe.
    map: std::sync::Mutex<evmap::WriteHandle<Bytes, Box<Metadata>>>,
    node_name: String,
    token: Option<String>,
    host: String,
    port: String,
    client: hyper::Client<HttpsConnector<HttpConnector<Resolver>>>,
}

impl MetadataClient {
    fn new(
        trans_config: &KubePodMetadata,
        map: evmap::WriteHandle<Bytes, Box<Metadata>>,
        resolver: Resolver,
        node_name: String,
        token: Option<String>,
        host: String,
        port: String,
        tls_settings: TlsSettings,
    ) -> Result<Self, BuildError> {
        // Select Pod metadata fields which are extracted and then added to Events.
        let fields = all_fields()
            .into_iter()
            .filter(|(key, _)| {
                trans_config
                    .fields
                    .iter()
                    .any(|field| field.as_str() == *key)
            })
            .map(|(_, fun)| fun)
            .collect();

        Ok(Self {
            fields,
            map: std::sync::Mutex::new(map),
            node_name,
            token,
            host,
            port,
            client: https_client(resolver, tls_settings).context(HttpError)?,
        })
    }

    /// Watches for metadata changes and propagates them to Transform.
    /// Returns only if it would always error.
    async fn run(&mut self) -> BuildError {
        // If watch is initiated with None resource_version, we will receive initial
        // list of pods as synthetic "Added" events.
        // https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
        let mut version = None;

        // Restarts watch request
        loop {
            // We could clear Metadata map at this point, as Kubernets documentation suggests,
            // but then we would have a time gap during which events wouldn't be enriched
            // with metadata.
            match self.watch_pods_request(version.clone()) {
                Ok(request) => version = self.watch(version, request).await,
                Err(error) => return error,
            }
        }
    }

    /// Watches for pods metadata with given watch request.
    async fn watch(
        &mut self,
        mut version: Option<Version>,
        request: Request<Body>,
    ) -> Option<Version> {
        // Start watching
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
                                    match WatchPodForAllNamespacesResponse::try_from_parts(
                                        status, &unused,
                                    ) {
                                        Ok((data, used_bytes)) => {
                                            // Process watch event
                                            match self.process_event(data) {
                                                WatchResult::New(new_version) => {
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
                                                WatchResult::Reload => (),
                                                WatchResult::Restart => return None,
                                            }
                                        }
                                        Err(ResponseError::NeedMoreData) => continue 'watch,
                                        Err(error) => debug!(
                                            "Unable to parse WatchPodForAllNamespacesResponse from response. Error: {:?}",
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

    /// Processes watch event comming from Kubernetes API server.
    fn process_event(&mut self, response: WatchPodForAllNamespacesResponse) -> WatchResult {
        match response {
            WatchPodForAllNamespacesResponse::Ok(event) => {
                match event {
                    WatchEvent::Added(pod)
                    | WatchEvent::Modified(pod)
                    | WatchEvent::Bookmark(pod)
                    | WatchEvent::Deleted(pod) => {
                        // In the case of Deleted, we don't delete it's data, as there could still exist unprocessed logs from that pod.
                        // Not deleteing it will cause "memory leakage" in a sense that the data won't be used ever
                        // again after some point, but the catch is that we don't know when that point is.
                        // Also considering that, on average, an entry occupies ~232B, so to 'leak' 1MB of memory, ~4500 pods would need to be
                        // created and destroyed on the same node, which is highly unlikely.
                        //
                        // An alternative would be to delay deletions of entrys by 1min. Which is a safe guess.

                        WatchResult::New(self.update(pod))
                    }
                    WatchEvent::ErrorStatus(status) => {
                        if status.code == Some(410) {
                            // 410 Gone, restart with new list.
                            // https://kubernetes.io/docs/reference/using-api/api-concepts/#410-gone-responses
                            warn!(message = "Pod list desynced. Reseting list.", cause = ?status);
                            WatchResult::Restart
                        } else {
                            debug!("Watch event with error status: {:?}", status);
                            WatchResult::New(None)
                        }
                    }
                    WatchEvent::ErrorOther(value) => {
                        debug!(?value);
                        WatchResult::New(None)
                    }
                }
            }
            WatchPodForAllNamespacesResponse::Other(Ok(_)) => {
                debug!(message = "Received wrong object from Kubernetes API.");
                WatchResult::New(None)
            }
            WatchPodForAllNamespacesResponse::Other(Err(error)) => {
                debug!(message = "Failed parsing watch list of Pods.", ?error);
                WatchResult::Reload
            }
        }
    }

    // Builds request to watch pods.
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

    // Selector for current Node.
    fn field_selector(&self) -> String {
        format!("spec.nodeName={}", self.node_name)
    }

    /// Extracts metadata from pod and sets them to map.
    fn update(&mut self, pod: Pod) -> Option<Version> {
        if let Some(uid) = pod.metadata.as_ref().and_then(|md| md.uid.as_ref()) {
            let uid: Bytes = uid.as_str().into();
            let fields = self.fields(&pod);

            // Update
            let map = self
                .map
                .get_mut()
                .expect("This is the only place making access.");
            map.update(uid, Box::new(fields));
            map.refresh();
        }

        pod.metadata
            .as_ref()
            .and_then(|metadata| metadata.resource_version.clone().map(Version))
    }

    /// Returns field values for given pod.
    fn fields(&self, pod: &Pod) -> Metadata {
        Metadata(self.fields.iter().flat_map(|fun| fun(pod).0).collect())
    }
}

/// Version of Kubernetes resource
#[derive(Clone, Debug)]
struct Version(String);

#[derive(Clone, Debug)]
enum WatchResult {
    /// Potentialy newer version
    New(Option<Version>),
    /// Start new request with current version.
    Reload,
    /// Start new request with None version.
    Restart,
}

#[derive(Clone, PartialEq)]
struct Metadata(Vec<(Atom, Value)>);

impl Eq for Metadata {}

impl Metadata {
    fn enrich(&self, event: &mut LogEvent) {
        for (key, value) in self.0.iter() {
            event.insert(key.clone(), value.clone());
        }
    }
}

pub struct KubernetesPodMetadata {
    metadata: evmap::ReadHandle<Bytes, Box<Metadata>>,
}

impl KubernetesPodMetadata {
    fn new(metadata: evmap::ReadHandle<Bytes, Box<Metadata>>) -> Self {
        Self { metadata }
    }
}

impl Transform for KubernetesPodMetadata {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();

        if let Some(Value::Bytes(pod_uid)) = log.get(&OBJECT_UID) {
            let pod_uid = pod_uid.clone();
            if self
                .metadata
                .get_and(&pod_uid, |fields| fields[0].enrich(log))
                .is_none()
            {
                warn!(
                    message = "Metadata for pod is not yet available.",
                    pod_uid = ?std::str::from_utf8(pod_uid.as_ref()),
                    rate_limit_secs = 10
                );
            }
        } else {
            warn!(
                message = "Event is without field.",
                field = OBJECT_UID.as_ref(),
                rate_limit_secs = 10
            );
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
    Box<dyn Fn(&Pod) -> Metadata + Send + Sync + 'static>,
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

fn field<T: Into<Value>>(
    name: &'static str,
    fun: impl Fn(&Pod) -> Option<T> + Send + Sync + 'static,
) -> (
    &'static str,
    Box<dyn Fn(&Pod) -> Metadata + Send + Sync + 'static>,
) {
    let key: Atom = with_prefix(name).into();
    let fun = move |pod: &Pod| {
        Metadata(
            fun(pod)
                .map(|data| vec![(key.clone(), data.into())])
                .unwrap_or_default(),
        )
    };
    (name, Box::new(fun) as Box<_>)
}

fn collection_field(
    name: &'static str,
    fun: impl Fn(&Pod) -> Option<&BTreeMap<String, String>> + Send + Sync + 'static,
) -> (
    &'static str,
    Box<dyn Fn(&Pod) -> Metadata + Send + Sync + 'static>,
) {
    let prefix_key = with_prefix(name) + ".";
    let fun = move |pod: &Pod| {
        Metadata(
            fun(pod)
                .map(|map| {
                    map.iter()
                        .map(|(key, value)| ((prefix_key.clone() + key).into(), value.into()))
                        .collect()
                })
                .unwrap_or_default(),
        )
    };
    (name, Box::new(fun) as Box<_>)
}

fn with_prefix(name: &str) -> String {
    FIELD_PREFIX.to_owned() + name
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "kubernetes-integration-tests")]

    use crate::sources::kubernetes::test::{echo, logs, user_namespace, Kube, VECTOR_YAML};
    use kube::api::{Api, RawApi};
    use uuid::Uuid;

    static NAME_MARKER: &'static str = "$(NAME)";
    static FIELD_MARKER: &'static str = "$(FIELD)";

    static ROLE_BINDING_YAML: &'static str = r#"
# Permissions to use Kubernetes API.
# Necessary for kubernetes_pod_metadata transform.
# Requires that RBAC authorization is enabled.
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: $(NAME)
subjects:
- kind: ServiceAccount
  name: default
  namespace: $(TEST_NAMESPACE)
roleRef:
  kind: ClusterRole
  name: view
  apiGroup: rbac.authorization.k8s.io
"#;

    static CONFIG_MAP_YAML_WITH_METADATA: &'static str = r#"
# ConfigMap which contains vector.toml configuration for pods.
apiVersion: v1
kind: ConfigMap
metadata:
  name: vector-config
  namespace: $(TEST_NAMESPACE)
data:
  vector-agent-config: |
    # VECTOR.TOML
    # Configuration for vector-agent

    # Set global options
    data_dir = "/tmp/vector/"

    # Ingest logs from Kubernetes
    [sources.kubernetes_logs]
      type = "kubernetes"

    [transforms.kube_metadata]
      type = "kubernetes_pod_metadata"
      inputs = ["kubernetes_logs"]
      $(FIELD)

    [sinks.out]
      type = "console"
      inputs = ["kube_metadata"]
      target = "stdout"

      encoding = "json"
      healthcheck = true

  # This line is not in VECTOR.TOML
"#;

    fn cluster_role_binding_api() -> RawApi {
        RawApi {
            group: "rbac.authorization.k8s.io".into(),
            resource: "clusterrolebindings".into(),
            prefix: "apis".into(),
            version: "v1".into(),
            ..Default::default()
        }
    }

    fn binding_name(namespace: &str) -> String {
        "binding-".to_owned() + namespace
    }

    fn metadata_config_map(fields: Option<Vec<&str>>) -> String {
        let replace = if let Some(fields) = fields {
            format!(
                "fields = [{}]",
                fields
                    .iter()
                    .map(|field| format!("{:?}", field))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        } else {
            "".to_owned()
        };

        CONFIG_MAP_YAML_WITH_METADATA.replace(FIELD_MARKER, replace.as_str())
    }

    #[test]
    fn kube_metadata() {
        let namespace = format!("kube-metadata-{}", Uuid::new_v4());
        let message = "20";
        let field = "node_name";
        let user_namespace = user_namespace(namespace.as_str());
        let binding_name = binding_name(namespace.as_str());

        let kube = Kube::new(namespace.as_str());
        let user = Kube::new(user_namespace.clone().as_str());

        // Cluster role binding
        kube.create_raw_with::<k8s_openapi::api::rbac::v1::ClusterRoleBinding>(
            &cluster_role_binding_api(),
            ROLE_BINDING_YAML
                .replace(NAME_MARKER, binding_name.as_str())
                .as_str(),
        );
        let _binding = kube.deleter(cluster_role_binding_api(), binding_name.as_str());

        // Start vector
        kube.create(
            Api::v1ConfigMap,
            metadata_config_map(Some(vec![field])).as_str(),
        );
        let vector = kube.create(Api::v1DaemonSet, VECTOR_YAML);

        // Wait for running state
        kube.wait_for_running(vector.clone());

        // Start echo
        let _echo = echo(&user, "echo", message);

        // Verify logs
        // If any daemon logged message, done.
        for line in logs(&kube, &vector) {
            if line.get(super::with_prefix(field)).is_some() {
                // DONE
                return;
            } else {
                debug!(namespace=namespace.as_str(),log=%line);
            }
        }
        panic!("Vector didn't find field: {:?}", field);
    }
}
