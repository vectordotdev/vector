use super::Transform;
use crate::{
    event::{Event, ValueKind},
    runtime::TaskExecutor,
    sources::kubernetes::POD_UID,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use bytes::Bytes;
use futures03::{compat::Future01CompatExt, stream::StreamExt};
use k8s_openapi::api::core::v1::PodSpec;
use kube::{
    self,
    api::{Api, WatchEvent},
    client::APIClient,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;
use tokio::timer::Delay;

/// Node name `spec.nodeName` of Vector pod passed down with Downward API.
const NODE_NAME_ENV: &str = "VECTOR_NODE_NAME";

/// Prefiks for all metadata fields
const FIELD_PREFIX: &str = "pod.";

type Pod = kube::api::Object<PodSpec, k8s_openapi::api::core::v1::PodStatus>;

/// Shared HashMap of (key,value) fields for pods on this node.
/// Joined on key - pod_uid field.
///
/// Mutex should work fine for this case.
type JoinMap = Arc<RwLock<HashMap<Bytes, Vec<(Atom, ValueKind)>>>>;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct KubernetesPodMetadataConfig {
    #[serde(default = "default_fields")]
    fields: Vec<String>,
}

inventory::submit! {
    TransformDescription::new_without_default::<KubernetesPodMetadataConfig>("kubernetes_pod_metadata")
}

#[typetag::serde(name = "kubernetes_pod_metadata")]
impl TransformConfig for KubernetesPodMetadataConfig {
    fn build(&self, exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        // Main idea is to have a background task which will premptively
        // acquire metadata for all pods on this node, and then maintaine that.

        let client = MetadataClient::new(node_name()?, self)?;
        let transform = KubernetesPodMetadata {
            metadata: client.metadata(),
        };
        exec.spawn_std(client.run());

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

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Kube errored: {}", source))]
    KubeError { source: kube::Error },
    #[snafu(display(
        "Missing environment variable {:?} containing node name `spec.nodeName`.",
        env
    ))]
    MissingNodeName { env: &'static str },
}

struct MetadataClient {
    fields: Vec<Box<dyn Fn(&Pod) -> Vec<(Atom, ValueKind)> + Send + Sync + 'static>>,
    metadata: JoinMap,
    node_name: String,
    kube: APIClient,
}

impl MetadataClient {
    /// Stream of regets of list.
    fn new(
        node_name: String,
        trans_config: &KubernetesPodMetadataConfig,
    ) -> Result<Self, BuildError> {
        let config = kube::config::incluster_config().context(KubeError)?;
        let kube = APIClient::new(config);

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
            kube,
        })
    }

    fn field_selector(&self) -> String {
        format!("spec.nodeName={}", self.node_name)
    }

    fn metadata(&self) -> JoinMap {
        self.metadata.clone()
    }

    async fn run(self) {
        loop {
            // Initialize metadata
            let list_version = self.fetch_pod_list().await;

            self.watch(list_version).await;
        }
    }

    /// list_version
    async fn fetch_pod_list(&self) -> String {
        loop {
            let r_list = Api::v1Pod(self.kube.clone())
                .list(&kube::api::ListParams {
                    field_selector: Some(self.field_selector()),
                    ..Default::default()
                })
                .await;

            match r_list {
                Ok(pod_list) => {
                    for pod in pod_list.items {
                        let _ = self.update(pod);
                    }

                    if let Some(version) = pod_list.metadata.resourceVersion {
                        return version;
                    }
                    debug!(message = "Missing pod list resource_version.")
                }
                Err(error) => debug!(message = "Failed fetching list of Pods.",error = ?error),
            }

            // Retry with delay
            Delay::new(Instant::now() + Duration::from_secs(1))
                .compat()
                .await
                .expect("Timer not set.");

            info!(message = "Re fetching list of Pods.");
        }
    }

    async fn watch(&self, version: String) {
        // Watch
        let informer = kube::api::Informer::new(Api::v1Pod(self.kube.clone()))
            .fields(&self.field_selector())
            .init_from(version);

        loop {
            let polled = informer.poll().await;
            match polled {
                Ok(mut stream) => {
                    for event in stream.collect::<Vec<_>>().await {
                        match event {
                            Ok(WatchEvent::Added(pod)) | Ok(WatchEvent::Modified(pod)) => {
                                let _ = self.update(pod);
                            }
                            // We do nothing, as there could still exist unprocessed logs from that pod.
                            Ok(WatchEvent::Deleted(_)) => (),
                            Ok(WatchEvent::Error(error)) => {
                                // 410 Gone, restart with new list.
                                if error.code == 410 {
                                    warn!("Reseting metadata because: {:?}", error);
                                    return;
                                }
                                debug!(?error)
                            }
                            Err(error) => debug!(?error),
                        }
                    }
                }
                Err(error) => debug!(?error),
            }
        }
    }

    fn update(&self, pod: Pod) -> Option<()> {
        let uid: Bytes = pod.metadata.uid.as_ref()?.as_str().into();

        let fields = self.fields(pod);

        trace!(message = "Updating Pod metadata.", uid = ?uid);

        // TODO: This is blocking
        let map = self.metadata.write().ok()?;

        map.insert(uid, fields);

        Some(())
    }

    fn fields(&self, pod: Pod) -> Vec<(Atom, ValueKind)> {
        self.fields.iter().flat_map(|fun| fun(&pod)).collect()
    }
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
        field("name", |pod| Some(pod.metadata.name.clone())),
        field("namespace", |pod| pod.metadata.namespace.clone()),
        field("creation_timestamp", |pod| {
            pod.metadata.creation_timestamp.clone().map(|time| time.0)
        }),
        field("deletion_timestamp", |pod| {
            pod.metadata.deletion_timestamp.clone().map(|time| time.0)
        }),
        collection_field("labels", |pod| &pod.metadata.labels),
        collection_field("annotations", |pod| &pod.metadata.annotations),
        // ------------------------ PodSpec ------------------------ //
        field("node_name", |pod| pod.spec.node_name.clone()),
        field("hostname", |pod| pod.spec.hostname.clone()),
        field("priority", |pod| pod.spec.priority),
        field("priority_class_name", |pod| {
            pod.spec.priority_class_name.clone()
        }),
        field("service_account_name", |pod| {
            pod.spec.service_account_name.clone()
        }),
        field("subdomain", |pod| pod.spec.subdomain.clone()),
        // ------------------------ PodStatus ------------------------ //
        field("host_ip", |pod| pod.status?.host_ip.clone()),
        field("ip", |pod| pod.status?.pod_ip.clone()),
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
    fun: impl Fn(&Pod) -> &BTreeMap<String, String> + Send + Sync + 'static,
) -> (
    &'static str,
    Box<dyn Fn(&Pod) -> Vec<(Atom, ValueKind)> + Send + Sync + 'static>,
) {
    let prefix_key = with_prefix(name) + ".";
    let fun = move |pod: &Pod| {
        fun(pod)
            .iter()
            .map(|(key, value)| ((prefix_key.clone() + key).into(), value.into()))
            .collect()
    };
    (name, Box::new(fun) as Box<_>)
}

fn with_prefix(name: &str) -> String {
    FIELD_PREFIX.to_owned() + name
}
