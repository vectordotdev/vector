pub mod watch_client;

use self::watch_client::{ClientConfig, PodEvent, RuntimeError, Version, WatchClient};
use super::Transform;
use crate::{
    event::{self, Event, Value},
    sources::kubernetes::POD_UID,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use bytes::Bytes;
use evmap::{ReadHandle, WriteHandle};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    stream::StreamExt,
};
use k8s_openapi::api::core::v1::Pod;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;
use tokio::timer::Delay;

// *********************** Defined by Vector **************************** //
/// Node name `spec.nodeName` of Vector pod passed down with Downward API.
const NODE_NAME_ENV: &str = "VECTOR_NODE_NAME";

/// If watcher errors, for how long will we wait before trying again.
const RETRY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct KubePodMetadata {
    #[serde(default = "default_fields")]
    fields: Vec<String>,
    /// For how long will we hold on to pod's metadata after the pod has been deleted.
    #[serde(default = "default_cache_ttl")]
    cache_ttl: u64,
}

fn default_cache_ttl() -> u64 {
    // 1h sounds a lot, but a metadata entry averages around 300B and in an extreme
    // scenario when a pod is deleted every second, we would have ~1MB of probably
    // not usefull data. Which is acceptable. The benefit of such a long delay
    // is that we will certainly* have the metadata while we are processing the
    // remaining logs from the deleted pod.
    60 * 60
}

inventory::submit! {
    TransformDescription::new_without_default::<KubePodMetadata>("kubernetes_pod_metadata")
}

#[typetag::serde(name = "kubernetes_pod_metadata")]
impl TransformConfig for KubePodMetadata {
    fn build(&self, cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        // Main idea is to have a background task which will premptively
        // acquire metadata for all pods on this node, and with it maintaine
        // a map of extracted metadata.

        // Construct WatchClient
        let node = std::env::var(NODE_NAME_ENV)
            .map_err(|_| BuildError::MissingNodeName { env: NODE_NAME_ENV })?;
        let wc_config = ClientConfig::in_cluster(node, cx.resolver()).context(WatchClientBuild)?;
        let watch_client = wc_config.build().context(WatchClientBuild)?;

        // Construct MetadataClient
        let (reader, writer) = evmap::new();
        let metadata_client = MetadataClient::new(self, writer, watch_client)?;

        // Run background task
        cx.executor().spawn_std(async move {
            match metadata_client.run().await {
                Ok(()) => unreachable!(),
                Err(error) => error!(message = "Stopped updating Pod metadata.", reason = %error),
            }
        });

        // Construct transform
        Ok(Box::new(KubernetesPodMetadata { metadata: reader }))
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

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed building WatchClient: {}", source))]
    WatchClientBuild {
        source: self::watch_client::BuildError,
    },
    #[snafu(display("Failed building watch stream: {}", source))]
    WatchStreamBuild {
        source: self::watch_client::BuildError,
    },
    #[snafu(display(
        "Missing environment variable {:?} containing node name `spec.nodeName`.",
        env
    ))]
    MissingNodeName { env: &'static str },
    #[snafu(display("Unknown metadata fields: {:?}", fields))]
    UnknownFields { fields: Vec<String> },
}

/// Vlient which watches for Pod metadata changes, extracts fields,
/// and writes them to the metadata map.
struct MetadataClient {
    fields: Vec<Box<dyn Fn(&Pod) -> Vec<(Atom, Value)> + Send + Sync + 'static>>,
    metadata: WriteHandle<Bytes, Box<(Atom, FieldValue)>>,
    /// (key of data to be deleted, can be deleted after this point in time)
    delete_queue: VecDeque<(Bytes, Instant)>,
    client: WatchClient,
    cache_ttl: Duration,
}

impl MetadataClient {
    fn new(
        config: &KubePodMetadata,
        metadata: WriteHandle<Bytes, Box<(Atom, FieldValue)>>,
        client: WatchClient,
    ) -> Result<Self, BuildError> {
        // Select Pod metadata fields that need to be extracted from
        // Pod and then added to Events.
        let mut add_fields = config.fields.clone();
        add_fields.sort();
        add_fields.dedup();

        let mut fields = Vec::new();
        let mut unknown = Vec::new();
        let mut all_fields = all_fields();

        for field in add_fields {
            if let Some(fun) = all_fields.remove(field.as_str()) {
                fields.push(fun);
            } else {
                unknown.push(field.to_owned());
            }
        }

        if unknown.is_empty() {
            Ok(Self {
                fields,
                metadata,
                client,
                delete_queue: VecDeque::new(),
                cache_ttl: Duration::from_secs(config.cache_ttl),
            })
        } else {
            Err(BuildError::UnknownFields { fields: unknown })
        }
    }

    /// Listens for pod metadata changes and updates metadata map.
    async fn run(mut self) -> Result<(), BuildError> {
        let mut version = None;
        let mut error = None;
        loop {
            // Build watcher stream
            let mut watcher = self
                .client
                .watch_metadata(version.clone(), error.take())
                .context(WatchStreamBuild)?
                .compat();
            info!("Watching Pod metadata.");

            // Watch loop
            error = Some(RuntimeError::WatchUnexpectedlyEnded);
            while let Some(next) = watcher.next().await {
                match next {
                    Ok(event) => {
                        self.delete_update();
                        version = self.update(event).or(version);

                        self.metadata.refresh();
                    }
                    Err(err) => {
                        error = Some(err);
                        break;
                    }
                }
            }

            warn!(
                message = "Temporary stoped watching Pod metadata.",
                reason = ?error
            );

            // Wait for bit before trying to watch again.
            let _ = Delay::new(Instant::now() + RETRY_TIMEOUT)
                .compat()
                .await
                .expect("Timer not set.");
        }
    }

    // In the case of Deleted, we don't delete it's data, as there could still exist unprocessed logs from that pod.
    // Not deleting it will cause "memory leakage" in a sense that the data won't be used ever
    // again after some point, but the catch is that we don't know when that point is.
    // Also considering that, on average, an entry occupies ~232B, so to 'leak' 1MB of memory, ~4500 pods would need to be
    // created and destroyed on the same node, which is highly unlikely.
    //
    // An alternative would be to delay deletions of entrys by 1min. Which is a safe guess.
    //
    /// Extracts metadata from pod and updates metadata map.
    fn update(&mut self, (pod, event): (Pod, PodEvent)) -> Option<Version> {
        if let Some(pod_uid) = pod.metadata.as_ref().and_then(|md| md.uid.as_ref()) {
            let uid: Bytes = pod_uid.as_str().into();

            self.metadata.clear(uid.clone());

            // Insert field values for this pod.
            for (field, value) in self.fields.iter().flat_map(|fun| fun(&pod)) {
                self.metadata
                    .insert(uid.clone(), Box::new((field, FieldValue(value))));
            }

            trace!(message = "Pod updated.", %pod_uid);

            if PodEvent::Deleted == event {
                self.delete_queue
                    .push_back((uid, Instant::now() + self.cache_ttl));
            }
        }

        Version::from_pod(&pod)
    }

    /// Checks if there are entries to be deleted.
    fn delete_update(&mut self) {
        let now = Instant::now();
        while let Some((pod_uid, deadline)) = self.delete_queue.get(0) {
            if *deadline <= now {
                self.metadata.empty(pod_uid.clone());

                trace!(message = "Pod metadata deleted.", pod_uid=?std::str::from_utf8(pod_uid.as_ref()));

                self.delete_queue.pop_front();
            } else {
                break;
            }
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
struct FieldValue(Value);

// Since we aren't using Eq feature in the evmap, we can impl Eq.
impl Eq for FieldValue {}

pub struct KubernetesPodMetadata {
    metadata: ReadHandle<Bytes, Box<(Atom, FieldValue)>>,
}

impl Transform for KubernetesPodMetadata {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();

        if let Some(Value::Bytes(pod_uid)) = log.get(&POD_UID) {
            let pod_uid = pod_uid.clone();

            let found = self.metadata.get_and(&pod_uid, |fields| {
                for pair in fields {
                    log.insert(pair.0.clone(), (pair.1).0.clone());
                }
            });

            if found.is_none() {
                warn!(
                    message = "Metadata for pod not yet available.",
                    pod_uid = ?std::str::from_utf8(pod_uid.as_ref()),
                    rate_limit_secs = 10
                );
            }
        } else {
            warn!(
                message = "Event without field, so it can't be enriched with metadata.",
                field = POD_UID.as_ref(),
                rate_limit_secs = 10
            );
        }

        Some(event)
    }
}

/// Default included fields
fn default_fields() -> Vec<String> {
    vec!["name", "namespace", "labels", "annotations", "node_name"]
        .into_iter()
        .map(Into::into)
        .collect()
}

/// Returns list of all supported fields and their extraction function.
fn all_fields(
) -> HashMap<&'static str, Box<dyn Fn(&Pod) -> Vec<(Atom, Value)> + Send + Sync + 'static>> {
    // Support for new fields can be added by adding them in the bellow vector.
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
    .into_iter()
    .collect()
}

fn field<T: Into<Value>>(
    name: &'static str,
    fun: impl Fn(&Pod) -> Option<T> + Send + Sync + 'static,
) -> (
    &'static str,
    Box<dyn Fn(&Pod) -> Vec<(Atom, Value)> + Send + Sync + 'static>,
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
    Box<dyn Fn(&Pod) -> Vec<(Atom, Value)> + Send + Sync + 'static>,
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
    event::log_schema().kubernetes_key().as_ref().to_owned() + "." + name
}

#[cfg(test)]
mod tests {
    use super::{KubePodMetadata, TransformConfig, TransformContext};
    use crate::test_util::runtime;

    #[test]
    fn unknown_fields() {
        let config = KubePodMetadata {
            fields: vec!["unknown".to_owned()],
        };

        let rt = runtime();

        assert!(config
            .build(TransformContext::new_test(rt.executor()))
            .is_err());
    }
}

#[cfg(test)]
mod integration_tests {
    #![cfg(feature = "kubernetes-integration-tests")]

    use crate::sources::kubernetes::test::{echo, logs, user_namespace, Kube, VECTOR_YAML};
    use crate::test_util::wait_for;
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

        // Add Vector configuration
        kube.create(
            Api::v1ConfigMap,
            metadata_config_map(Some(vec![field])).as_str(),
        );

        // Start vector
        let vector = kube.create(Api::v1DaemonSet, VECTOR_YAML);

        // Wait for running state
        kube.wait_for_running(vector.clone());

        // Start echo
        let _echo = echo(&user, "echo", message);

        // Verify logs
        wait_for(|| {
            // If any daemon logged message, done.
            for line in logs(&kube, &vector) {
                if line
                    .get(crate::event::log_schema().kubernetes_key().as_ref())
                    .and_then(|kube| kube.get(field))
                    .is_some()
                {
                    // DONE
                    return true;
                } else {
                    debug!(namespace=namespace.as_str(),log=%line);
                }
            }
            false
        });
    }
}
