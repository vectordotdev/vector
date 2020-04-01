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
    future::{select, Either},
    stream::StreamExt,
};
use k8s_openapi::api::core::v1::Pod;
use rand::Rng;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};
use string_cache::DefaultAtom as Atom;
use tokio01::timer::Delay;

/// Node name `spec.nodeName` of Vector pod passed down with Downward API.
const NODE_NAME_ENV: &str = "VECTOR_NODE_NAME";

/// 1h sounds a lot, but a metadata entry averages around 300B and in an extreme
/// scenario when a pod is deleted every second, we would have ~1MB of probably
/// not usefull data. Which is acceptable. The benefit of such a long delay
/// is that we will certainly* have the metadata while we are processing the
/// remaining logs from the deleted pod.
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Must be larger than 0.
const DEFAULT_MAX_RETRY_TIMEOUT: Duration = Duration::from_secs(1);

/// Default fields added to events
const DEFAULT_FIELDS: [Field; 5] = [
    Field::Name,
    Field::Namespace,
    Field::Labels,
    Field::Annotations,
    Field::NodeName,
];

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct KubePodMetadata {
    fields: Option<Vec<Field>>,
    /// For how long will we hold on to pod's metadata after the pod has been deleted.
    /// seconds
    cache_ttl: Option<u64>,
    /// Node name whose pod's metadata should be watched. Has priority
    /// over value found in enviroment variable.
    node_name: Option<String>,
    /// Field containg Pod UID to which log belongs.
    pod_uid: Option<String>,
    /// If watcher errors, for how maximaly will we wait before trying again.
    /// Must be larger than 0.
    /// seconds
    max_retry_timeout: Option<u64>,
}

inventory::submit! {
    TransformDescription::new_without_default::<KubePodMetadata>("kubernetes_pod_metadata")
}

#[typetag::serde(name = "kubernetes_pod_metadata")]
impl TransformConfig for KubePodMetadata {
    fn build(&self, cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        // Main idea is to have a background task which will premptively
        // acquire metadata for all pods on this node, and with it maintain
        // a map of extracted metadata.

        // Detemine Node's name of whose Pod's metadata we will watch for.
        let node = if let Some(node) = self.node_name.clone() {
            node
        } else {
            std::env::var(NODE_NAME_ENV)
                .map_err(|_| BuildError::MissingNodeName { env: NODE_NAME_ENV })?
        };

        // Construct WatchClient
        let wc_config = ClientConfig::in_cluster(node, cx.resolver()).context(WatchClientBuild)?;
        let watch_client = wc_config.build().context(WatchClientBuild)?;

        // Construct MetadataClient
        let (reader, writer) = evmap::new();
        let metadata_client = MetadataClient::new(
            self.fields
                .clone()
                .unwrap_or(DEFAULT_FIELDS.iter().map(Clone::clone).collect()),
            self.cache_ttl
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_CACHE_TTL),
            self.max_retry_timeout
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_MAX_RETRY_TIMEOUT),
            writer,
            watch_client,
        )?;

        // Run background task
        cx.executor().spawn_std(async move {
            match metadata_client.run().await {
                Ok(()) => (),
                Err(error) => error!(message = "Stopped updating Pod metadata.", reason = %error),
            }
        });

        // Construct transform
        Ok(Box::new(KubernetesPodMetadata {
            metadata: reader,
            pod_uid: self
                .pod_uid
                .clone()
                .map(Into::into)
                .unwrap_or(POD_UID.clone()),
        }))
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
    #[snafu(display("max_retry_timeout must be larger than 0."))]
    TooSmallMaxRetryTimeout,
}

/// Client which watches for Pod metadata changes, extracts fields,
/// and writes them to the metadata map.
struct MetadataClient {
    fields: Vec<Field>,
    metadata: WriteHandle<Bytes, Box<(Atom, FieldValue)>>,
    /// (key of data to be deleted, can be deleted after this point in time)
    delete_queue: VecDeque<(Bytes, Instant)>,
    client: WatchClient,
    cache_ttl: Duration,
    max_retry_timeout: Duration,
}

impl MetadataClient {
    fn new(
        fields: Vec<Field>,
        cache_ttl: Duration,
        max_retry_timeout: Duration,
        metadata: WriteHandle<Bytes, Box<(Atom, FieldValue)>>,
        client: WatchClient,
    ) -> Result<Self, BuildError> {
        if max_retry_timeout > Duration::default() {
            Ok(Self {
                fields,
                metadata,
                client,
                delete_queue: VecDeque::new(),
                cache_ttl,
                max_retry_timeout,
            })
        } else {
            Err(BuildError::TooSmallMaxRetryTimeout)
        }
    }

    /// Listens for pod metadata changes and updates metadata map.
    async fn run(mut self) -> Result<(), BuildError> {
        let mut version = None;
        let mut error = None;
        // Since this transform will in most cases be deployed on all Nodes
        // and fairly simultaneously, we can immediately start with user
        // defined max_retry_timeout.
        let mut retry_timeout = self.max_retry_timeout;
        loop {
            // Wait for bit before trying to watch again.
            // We are sampling from a uniform distribution here.
            let timeout = rand::thread_rng().gen_range(Duration::default(), retry_timeout);
            let _ = Delay::new(Instant::now() + timeout)
                .compat()
                .await
                .expect("Timer not set.");

            // Build watcher stream
            let mut watcher = self
                .client
                .watch_metadata(version.clone(), error.take())
                .context(WatchStreamBuild)?
                .compat();
            info!("Watching Pod metadata.");

            // Watch loop
            let mut runtime_error = RuntimeError::WatchUnexpectedlyEnded;
            retry_timeout = self.max_retry_timeout.min(retry_timeout * 2);

            let mut watch = watcher.next();
            loop {
                let either = select(
                    watch,
                    Delay::new(Instant::now() + Duration::from_secs(1)).compat(),
                )
                .await;

                self.delete_update();

                match either {
                    Either::Left((next, _)) => match next {
                        Some(Ok(event)) => {
                            version = self.update(event).or(version);
                            watch = watcher.next();
                        }
                        Some(Err(err)) => {
                            match err {
                                // Keep the retry_timeout for errors that could
                                // be caused by the api server being overloaded.
                                RuntimeError::FailedConnecting { .. }
                                | RuntimeError::ConnectionStatusNotOK { .. }
                                | RuntimeError::WatchConnectionErrored { .. }
                                | RuntimeError::WatchUnexpectedlyEnded => (),
                                // Optimistically try the shortest timeout.
                                _ => retry_timeout = Duration::from_secs(1),
                            }
                            runtime_error = err;
                            break;
                        }
                        None => break,
                    },
                    Either::Right((_, rewatch)) => watch = rewatch,
                }
            }

            warn!(
                message = "Temporarily stoped watching Pod metadata.",
                reason = %runtime_error
            );

            error = Some(runtime_error);
        }
    }

    /// Extracts metadata from pod and updates metadata map.
    fn update(&mut self, (pod, event): (Pod, PodEvent)) -> Option<Version> {
        if let Some(pod_uid) = pod.metadata.as_ref().and_then(|md| md.uid.as_ref()) {
            let uid: Bytes = pod_uid.as_str().into();

            self.metadata.clear(uid.clone());

            // Insert field values for this pod.
            for (field, value) in self.fields.iter().filter_map(|field| field.extract(&pod)) {
                self.metadata
                    .insert(uid.clone(), Box::new((field, FieldValue(value))));
            }

            trace!(message = "Pod updated.", %pod_uid);

            if PodEvent::Deleted == event {
                self.delete_queue
                    .push_back((uid, Instant::now() + self.cache_ttl));
            }

            self.metadata.refresh();
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
        self.metadata.refresh();
    }
}

#[derive(PartialEq, Debug, Clone)]
struct FieldValue(Value);

// Since we aren't using Eq feature in the evmap, we can impl Eq.
impl Eq for FieldValue {}

pub struct KubernetesPodMetadata {
    metadata: ReadHandle<Bytes, Box<(Atom, FieldValue)>>,
    pod_uid: Atom,
}

impl Transform for KubernetesPodMetadata {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();

        if let Some(Value::Bytes(pod_uid)) = log.get(&self.pod_uid) {
            let pod_uid = pod_uid.clone();

            let found = self.metadata.get_and(&pod_uid, |fields| {
                for pair in fields {
                    log.insert(pair.0.clone(), (pair.1).0.clone());
                }
            });

            if found.is_none() {
                warn!(
                    message = "Failed enriching Event.",
                    pod_uid = ?std::str::from_utf8(pod_uid.as_ref()),
                    error = "Metadata for pod is not available.",
                    rate_limit_secs = 30
                );
            }
        } else {
            warn!(
                message = "Failed enriching Event.",
                field = self.pod_uid.as_ref(),
                error = "Missing field.",
                rate_limit_secs = 30
            );
        }

        Some(event)
    }
}

/// Extractable fields
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Field {
    Name,
    Namespace,
    CreationTimestamp,
    DeletionTimestamp,
    Labels,
    Annotations,
    NodeName,
    Hostname,
    Priority,
    PriorityClassName,
    ServiceAccountName,
    Subdomain,
    HostIp,
    Ip,
}

impl Field {
    fn name(self) -> String {
        toml::to_string(&self).unwrap().replace('\"', "")
    }

    /// Extracts this field from Pod
    fn extract(self, pod: &Pod) -> Option<(Atom, Value)> {
        use Field::*;

        Some(match self {
            // ------------------------ ObjectMeta ------------------------ //
            Name => self.field(pod.metadata.as_ref()?.name.clone()?),
            Namespace => self.field(pod.metadata.as_ref()?.namespace.clone()?),
            CreationTimestamp => self.field(pod.metadata.as_ref()?.creation_timestamp.clone()?.0),
            DeletionTimestamp => self.field(pod.metadata.as_ref()?.deletion_timestamp.clone()?.0),
            Labels => self.collection(pod.metadata.as_ref()?.labels.as_ref()?),
            Annotations => self.collection(pod.metadata.as_ref()?.annotations.as_ref()?),
            // ------------------------ PodSpec ------------------------ //
            NodeName => self.field(pod.spec.as_ref()?.node_name.clone()?),
            Hostname => self.field(pod.spec.as_ref()?.hostname.clone()?),
            Priority => self.field(pod.spec.as_ref()?.priority.clone()?),
            PriorityClassName => self.field(pod.spec.as_ref()?.priority_class_name.clone()?),
            ServiceAccountName => self.field(pod.spec.as_ref()?.service_account_name.clone()?),
            Subdomain => self.field(pod.spec.as_ref()?.subdomain.clone()?),
            // ------------------------ PodStatus ------------------------ //
            HostIp => self.field(pod.status.as_ref()?.host_ip.clone()?),
            Ip => self.field(pod.status.as_ref()?.pod_ip.clone()?),
        })
    }

    fn field(self, data: impl Into<Value>) -> (Atom, Value) {
        (Self::with_prefix(&self.name()).into(), data.into())
    }

    fn collection(self, map: &BTreeMap<String, String>) -> (Atom, Value) {
        (
            Self::with_prefix(&self.name()).into(),
            Value::Map(
                map.iter()
                    .map(|(key, value)| (key.as_str().into(), value.into()))
                    .collect(),
            ),
        )
    }

    fn with_prefix(name: &str) -> String {
        event::log_schema().kubernetes_key().as_ref().to_owned() + "." + name
    }
}

#[cfg(test)]
mod tests {
    use super::Field;

    #[test]
    fn field_name() {
        assert_eq!("priority_class_name", &Field::PriorityClassName.name());
    }
}

#[cfg(test)]
mod integration_tests {
    #![cfg(feature = "kubernetes-integration-tests")]

    use crate::sources::kubernetes::test::{echo, logs, start_vector, user_namespace, Kube};
    use crate::test_util::{random_string, wait_for};
    use kube::api::RawApi;
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
      include_namespaces = ["$(USER_TEST_NAMESPACE)"]
      include_container_names = [$(USER_CONTAINERS)]
      include_pod_uids = [$(USER_POD_UIDS)]

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
        let message = random_string(300);
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
        let vector = start_vector(
            &kube,
            &user_namespace,
            None,
            metadata_config_map(Some(vec![field])).as_str(),
        );

        // Start echo
        let _echo = echo(&user, "echo", &message);

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
