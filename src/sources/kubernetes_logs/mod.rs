//! This mod implements `kubernetes_logs` source.
//! The scope of this source is to consume the log files that a kubelet keeps
//! at "/var/log/pods" on the host of the Kubernetes Node when Vector itself is
//! running inside the cluster as a DaemonSet.

#![deny(missing_docs)]
use std::{path::PathBuf, time::Duration};

use bytes::Bytes;
use chrono::Utc;
use futures::{future::FutureExt, stream::StreamExt};
use futures_util::Stream;
use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
use k8s_paths_provider::K8sPathsProvider;
use kube::{
    api::Api,
    config::{self, KubeConfigOptions},
    runtime::{reflector, watcher, WatchStreamExt},
    Client, Config as ClientConfig,
};
use lifecycle::Lifecycle;
use serde_with::serde_as;
use vector_lib::codecs::{BytesDeserializer, BytesDeserializerConfig};
use vector_lib::configurable::configurable_component;
use vector_lib::file_source::{
    calculate_ignore_before, Checkpointer, FileServer, FileServerShutdown, FingerprintStrategy,
    Fingerprinter, Line, ReadFrom, ReadFromConfig,
};
use vector_lib::lookup::{lookup_v2::OptionalTargetPath, owned_value_path, path, OwnedTargetPath};
use vector_lib::{config::LegacyKey, config::LogNamespace, EstimatedJsonEncodedSizeOf};
use vector_lib::{
    internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol},
    TimeZone,
};
use vrl::value::{kind::Collection, Kind};

use crate::sources::kubernetes_logs::partial_events_merger::merge_partial_events;
use crate::{
    config::{
        log_schema, ComponentKey, DataType, GenerateConfig, GlobalOptions, SourceConfig,
        SourceContext, SourceOutput,
    },
    event::Event,
    internal_events::{
        FileInternalMetricsConfig, FileSourceInternalEventsEmitter, KubernetesLifecycleError,
        KubernetesLogsEventAnnotationError, KubernetesLogsEventNamespaceAnnotationError,
        KubernetesLogsEventNodeAnnotationError, KubernetesLogsEventsReceived,
        KubernetesLogsPodInfo, StreamClosedError,
    },
    kubernetes::{custom_reflector, meta_cache::MetaCache},
    shutdown::ShutdownSignal,
    sources,
    transforms::{FunctionTransform, OutputBuffer},
    SourceSender,
};

mod k8s_paths_provider;
mod lifecycle;
mod namespace_metadata_annotator;
mod node_metadata_annotator;
mod parser;
mod partial_events_merger;
mod path_helpers;
mod pod_metadata_annotator;
mod transform_utils;
mod util;

use self::namespace_metadata_annotator::NamespaceMetadataAnnotator;
use self::node_metadata_annotator::NodeMetadataAnnotator;
use self::parser::Parser;
use self::pod_metadata_annotator::PodMetadataAnnotator;

/// The `self_node_name` value env var key.
const SELF_NODE_NAME_ENV_KEY: &str = "VECTOR_SELF_NODE_NAME";

/// Configuration for the `kubernetes_logs` source.
#[serde_as]
#[configurable_component(source("kubernetes_logs", "Collect Pod logs from Kubernetes Nodes."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Specifies the [label selector][label_selector] to filter [Pods][pods] with, to be used in
    /// addition to the built-in [exclude][exclude] filter.
    ///
    /// [label_selector]: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#label-selectors
    /// [pods]: https://kubernetes.io/docs/concepts/workloads/pods/
    /// [exclude]: https://vector.dev/docs/reference/configuration/sources/kubernetes_logs/#pod-exclusion
    #[configurable(metadata(docs::examples = "my_custom_label!=my_value"))]
    #[configurable(metadata(
        docs::examples = "my_custom_label!=my_value,my_other_custom_label=my_value"
    ))]
    extra_label_selector: String,

    /// Specifies the [label selector][label_selector] to filter [Namespaces][namespaces] with, to
    /// be used in addition to the built-in [exclude][exclude] filter.
    ///
    /// [label_selector]: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#label-selectors
    /// [namespaces]: https://kubernetes.io/docs/concepts/overview/working-with-objects/namespaces/
    /// [exclude]: https://vector.dev/docs/reference/configuration/sources/kubernetes_logs/#namespace-exclusion
    #[configurable(metadata(docs::examples = "my_custom_label!=my_value"))]
    #[configurable(metadata(
        docs::examples = "my_custom_label!=my_value,my_other_custom_label=my_value"
    ))]
    extra_namespace_label_selector: String,

    /// The name of the Kubernetes [Node][node] that is running.
    ///
    /// Configured to use an environment variable by default, to be evaluated to a value provided by
    /// Kubernetes at Pod creation.
    ///
    /// [node]: https://kubernetes.io/docs/concepts/architecture/nodes/
    self_node_name: String,

    /// Specifies the [field selector][field_selector] to filter Pods with, to be used in addition
    /// to the built-in [Node][node] filter.
    ///
    /// The built-in Node filter uses `self_node_name` to only watch Pods located on the same Node.
    ///
    /// [field_selector]: https://kubernetes.io/docs/concepts/overview/working-with-objects/field-selectors/
    /// [node]: https://kubernetes.io/docs/concepts/architecture/nodes/
    #[configurable(metadata(docs::examples = "metadata.name!=pod-name-to-exclude"))]
    #[configurable(metadata(
        docs::examples = "metadata.name!=pod-name-to-exclude,metadata.name=mypod"
    ))]
    extra_field_selector: String,

    /// Whether or not to automatically merge partial events.
    ///
    /// Partial events are messages that were split by the Kubernetes Container Runtime
    /// log driver.
    auto_partial_merge: bool,

    /// The directory used to persist file checkpoint positions.
    ///
    /// By default, the [global `data_dir` option][global_data_dir] is used.
    /// Make sure the running user has write permissions to this directory.
    ///
    /// If this directory is specified, then Vector will attempt to create it.
    ///
    /// [global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
    #[configurable(metadata(docs::examples = "/var/local/lib/vector/"))]
    #[configurable(metadata(docs::human_name = "Data Directory"))]
    data_dir: Option<PathBuf>,

    #[configurable(derived)]
    #[serde(alias = "annotation_fields")]
    pod_annotation_fields: pod_metadata_annotator::FieldsSpec,

    #[configurable(derived)]
    namespace_annotation_fields: namespace_metadata_annotator::FieldsSpec,

    #[configurable(derived)]
    node_annotation_fields: node_metadata_annotator::FieldsSpec,

    /// A list of glob patterns to include while reading the files.
    #[configurable(metadata(docs::examples = "**/include/**"))]
    include_paths_glob_patterns: Vec<PathBuf>,

    /// A list of glob patterns to exclude from reading the files.
    #[configurable(metadata(docs::examples = "**/exclude/**"))]
    exclude_paths_glob_patterns: Vec<PathBuf>,

    #[configurable(derived)]
    #[serde(default = "default_read_from")]
    read_from: ReadFromConfig,

    /// Ignore files with a data modification date older than the specified number of seconds.
    #[serde(default)]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 600))]
    #[configurable(metadata(docs::human_name = "Ignore Files Older Than"))]
    ignore_older_secs: Option<u64>,

    /// Max amount of bytes to read from a single file before switching over to the next file.
    /// **Note:** This does not apply when `oldest_first` is `true`.
    ///
    /// This allows distributing the reads more or less evenly across
    /// the files.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    max_read_bytes: usize,

    /// Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from more recent files.
    #[serde(default = "default_oldest_first")]
    pub oldest_first: bool,

    /// The maximum number of bytes a line can contain before being discarded.
    ///
    /// This protects against malformed lines or tailing incorrect files.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    max_line_bytes: usize,

    /// The number of lines to read for generating the checksum.
    ///
    /// If your files share a common header that is not always a fixed size,
    ///
    /// If the file has less than this amount of lines, it won’t be read at all.
    #[configurable(metadata(docs::type_unit = "lines"))]
    fingerprint_lines: usize,

    /// The interval at which the file system is polled to identify new files to read from.
    ///
    /// This is quite efficient, yet might still create some load on the
    /// file system; in addition, it is currently coupled with checksum dumping
    /// in the underlying file server, so setting it too low may introduce
    /// a significant overhead.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Glob Minimum Cooldown"))]
    glob_minimum_cooldown_ms: Duration,

    /// Overrides the name of the log field used to add the ingestion timestamp to each event.
    ///
    /// This is useful to compute the latency between important event processing
    /// stages. For example, the time delta between when a log line was written and when it was
    /// processed by the `kubernetes_logs` source.
    #[configurable(metadata(docs::examples = ".ingest_timestamp", docs::examples = "ingest_ts"))]
    ingestion_timestamp_field: Option<OptionalTargetPath>,

    /// The default time zone for timestamps without an explicit zone.
    timezone: Option<TimeZone>,

    /// Optional path to a readable [kubeconfig][kubeconfig] file.
    ///
    /// If not set, a connection to Kubernetes is made using the in-cluster configuration.
    ///
    /// [kubeconfig]: https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/
    #[configurable(metadata(docs::examples = "/path/to/.kube/config"))]
    kube_config_file: Option<PathBuf>,

    /// Determines if requests to the kube-apiserver can be served by a cache.
    use_apiserver_cache: bool,

    /// How long to delay removing metadata entries from the cache when a pod deletion event
    /// event is received from the watch stream.
    ///
    /// A longer delay allows for continued enrichment of logs after the originating Pod is
    /// removed. If relevant metadata has been removed, the log is forwarded un-enriched and a
    /// warning is emitted.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Delay Deletion"))]
    delay_deletion_ms: Duration,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default)]
    internal_metrics: FileInternalMetricsConfig,

    /// How long to keep an open handle to a rotated log file.
    /// The default value represents "no limit"
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(default = "default_rotate_wait", rename = "rotate_wait_secs")]
    rotate_wait: Duration,
}

const fn default_read_from() -> ReadFromConfig {
    ReadFromConfig::Beginning
}

impl GenerateConfig for Config {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            self_node_name: default_self_node_name_env_template(),
            auto_partial_merge: true,
            ..Default::default()
        })
        .unwrap()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            extra_label_selector: "".to_string(),
            extra_namespace_label_selector: "".to_string(),
            self_node_name: default_self_node_name_env_template(),
            extra_field_selector: "".to_string(),
            auto_partial_merge: true,
            data_dir: None,
            pod_annotation_fields: pod_metadata_annotator::FieldsSpec::default(),
            namespace_annotation_fields: namespace_metadata_annotator::FieldsSpec::default(),
            node_annotation_fields: node_metadata_annotator::FieldsSpec::default(),
            include_paths_glob_patterns: default_path_inclusion(),
            exclude_paths_glob_patterns: default_path_exclusion(),
            read_from: default_read_from(),
            ignore_older_secs: None,
            max_read_bytes: default_max_read_bytes(),
            oldest_first: default_oldest_first(),
            max_line_bytes: default_max_line_bytes(),
            fingerprint_lines: default_fingerprint_lines(),
            glob_minimum_cooldown_ms: default_glob_minimum_cooldown_ms(),
            ingestion_timestamp_field: None,
            timezone: None,
            kube_config_file: None,
            use_apiserver_cache: false,
            delay_deletion_ms: default_delay_deletion_ms(),
            log_namespace: None,
            internal_metrics: Default::default(),
            rotate_wait: default_rotate_wait(),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_logs")]
impl SourceConfig for Config {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let source = Source::new(self, &cx.globals, &cx.key).await?;

        Ok(Box::pin(
            source
                .run(cx.out, cx.shutdown, log_namespace)
                .map(|result| {
                    result.map_err(|error| {
                        error!(message = "Source future failed.", %error);
                    })
                }),
        ))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = BytesDeserializerConfig
            .schema_definition(log_namespace)
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("file"))),
                &owned_value_path!("file"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .container_id
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("container_id"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .container_image
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("container_image"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .container_name
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("container_name"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.namespace_annotation_fields
                    .namespace_labels
                    .path
                    .clone()
                    .map(|x| LegacyKey::Overwrite(x.path)),
                &owned_value_path!("namespace_labels"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.node_annotation_fields
                    .node_labels
                    .path
                    .clone()
                    .map(|x| LegacyKey::Overwrite(x.path)),
                &owned_value_path!("node_labels"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_annotations
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_annotations"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_ip
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_ip"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_ips
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_ips"),
                Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_labels
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_labels"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_name
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_name"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_namespace
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_namespace"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_node_name
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_node_name"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_owner
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_owner"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                self.pod_annotation_fields
                    .pod_uid
                    .path
                    .clone()
                    .map(|k| k.path)
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("pod_uid"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("stream"))),
                &owned_value_path!("stream"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                log_schema()
                    .timestamp_key()
                    .cloned()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone)]
struct Source {
    client: Client,
    data_dir: PathBuf,
    auto_partial_merge: bool,
    pod_fields_spec: pod_metadata_annotator::FieldsSpec,
    namespace_fields_spec: namespace_metadata_annotator::FieldsSpec,
    node_field_spec: node_metadata_annotator::FieldsSpec,
    field_selector: String,
    label_selector: String,
    namespace_label_selector: String,
    node_selector: String,
    self_node_name: String,
    include_paths: Vec<glob::Pattern>,
    exclude_paths: Vec<glob::Pattern>,
    read_from: ReadFrom,
    ignore_older_secs: Option<u64>,
    max_read_bytes: usize,
    oldest_first: bool,
    max_line_bytes: usize,
    fingerprint_lines: usize,
    glob_minimum_cooldown: Duration,
    use_apiserver_cache: bool,
    ingestion_timestamp_field: Option<OwnedTargetPath>,
    delay_deletion: Duration,
    include_file_metric_tag: bool,
    rotate_wait: Duration,
}

impl Source {
    async fn new(
        config: &Config,
        globals: &GlobalOptions,
        key: &ComponentKey,
    ) -> crate::Result<Self> {
        let self_node_name = if config.self_node_name.is_empty()
            || config.self_node_name == default_self_node_name_env_template()
        {
            std::env::var(SELF_NODE_NAME_ENV_KEY).map_err(|_| {
                format!(
                    "self_node_name config value or {} env var is not set",
                    SELF_NODE_NAME_ENV_KEY
                )
            })?
        } else {
            config.self_node_name.clone()
        };

        let field_selector = prepare_field_selector(config, self_node_name.as_str())?;
        let label_selector = prepare_label_selector(config.extra_label_selector.as_ref());
        let namespace_label_selector =
            prepare_label_selector(config.extra_namespace_label_selector.as_ref());
        let node_selector = prepare_node_selector(self_node_name.as_str())?;

        // If the user passed a custom Kubeconfig use it, otherwise
        // we attempt to load the local kubeconfig, followed by the
        // in-cluster environment variables
        let client_config = match &config.kube_config_file {
            Some(kc) => {
                ClientConfig::from_custom_kubeconfig(
                    config::Kubeconfig::read_from(kc)?,
                    &KubeConfigOptions::default(),
                )
                .await?
            }
            None => ClientConfig::infer().await?,
        };
        let client = Client::try_from(client_config)?;

        let data_dir = globals.resolve_and_make_data_subdir(config.data_dir.as_ref(), key.id())?;

        let include_paths = prepare_include_paths(config)?;

        let exclude_paths = prepare_exclude_paths(config)?;

        let glob_minimum_cooldown = config.glob_minimum_cooldown_ms;

        let delay_deletion = config.delay_deletion_ms;

        let ingestion_timestamp_field = config
            .ingestion_timestamp_field
            .clone()
            .and_then(|k| k.path);

        Ok(Self {
            client,
            data_dir,
            auto_partial_merge: config.auto_partial_merge,
            pod_fields_spec: config.pod_annotation_fields.clone(),
            namespace_fields_spec: config.namespace_annotation_fields.clone(),
            node_field_spec: config.node_annotation_fields.clone(),
            field_selector,
            label_selector,
            namespace_label_selector,
            node_selector,
            self_node_name,
            include_paths,
            exclude_paths,
            read_from: ReadFrom::from(config.read_from),
            ignore_older_secs: config.ignore_older_secs,
            max_read_bytes: config.max_read_bytes,
            oldest_first: config.oldest_first,
            max_line_bytes: config.max_line_bytes,
            fingerprint_lines: config.fingerprint_lines,
            glob_minimum_cooldown,
            use_apiserver_cache: config.use_apiserver_cache,
            ingestion_timestamp_field,
            delay_deletion,
            include_file_metric_tag: config.internal_metrics.include_file_tag,
            rotate_wait: config.rotate_wait,
        })
    }

    async fn run(
        self,
        mut out: SourceSender,
        global_shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
    ) -> crate::Result<()> {
        let Self {
            client,
            data_dir,
            auto_partial_merge,
            pod_fields_spec,
            namespace_fields_spec,
            node_field_spec,
            field_selector,
            label_selector,
            namespace_label_selector,
            node_selector,
            self_node_name,
            include_paths,
            exclude_paths,
            read_from,
            ignore_older_secs,
            max_read_bytes,
            oldest_first,
            max_line_bytes,
            fingerprint_lines,
            glob_minimum_cooldown,
            use_apiserver_cache,
            ingestion_timestamp_field,
            delay_deletion,
            include_file_metric_tag,
            rotate_wait,
        } = self;

        let mut reflectors = Vec::new();

        let pods = Api::<Pod>::all(client.clone());

        let list_semantic = if use_apiserver_cache {
            watcher::ListSemantic::Any
        } else {
            watcher::ListSemantic::MostRecent
        };

        let pod_watcher = watcher(
            pods,
            watcher::Config {
                field_selector: Some(field_selector),
                label_selector: Some(label_selector),
                list_semantic: list_semantic.clone(),
                ..Default::default()
            },
        )
        .backoff(watcher::default_backoff());
        let pod_store_w = reflector::store::Writer::default();
        let pod_state = pod_store_w.as_reader();
        let pod_cacher = MetaCache::new();

        reflectors.push(tokio::spawn(custom_reflector(
            pod_store_w,
            pod_cacher,
            pod_watcher,
            delay_deletion,
        )));

        // -----------------------------------------------------------------

        let namespaces = Api::<Namespace>::all(client.clone());
        let ns_watcher = watcher(
            namespaces,
            watcher::Config {
                label_selector: Some(namespace_label_selector),
                list_semantic: list_semantic.clone(),
                ..Default::default()
            },
        )
        .backoff(watcher::default_backoff());
        let ns_store_w = reflector::store::Writer::default();
        let ns_state = ns_store_w.as_reader();
        let ns_cacher = MetaCache::new();

        reflectors.push(tokio::spawn(custom_reflector(
            ns_store_w,
            ns_cacher,
            ns_watcher,
            delay_deletion,
        )));

        // -----------------------------------------------------------------

        let nodes = Api::<Node>::all(client);
        let node_watcher = watcher(
            nodes,
            watcher::Config {
                field_selector: Some(node_selector),
                list_semantic,
                ..Default::default()
            },
        )
        .backoff(watcher::default_backoff());
        let node_store_w = reflector::store::Writer::default();
        let node_state = node_store_w.as_reader();
        let node_cacher = MetaCache::new();

        reflectors.push(tokio::spawn(custom_reflector(
            node_store_w,
            node_cacher,
            node_watcher,
            delay_deletion,
        )));

        let paths_provider = K8sPathsProvider::new(
            pod_state.clone(),
            ns_state.clone(),
            include_paths,
            exclude_paths,
        );
        let annotator = PodMetadataAnnotator::new(pod_state, pod_fields_spec, log_namespace);
        let ns_annotator =
            NamespaceMetadataAnnotator::new(ns_state, namespace_fields_spec, log_namespace);
        let node_annotator = NodeMetadataAnnotator::new(node_state, node_field_spec, log_namespace);

        let ignore_before = calculate_ignore_before(ignore_older_secs);

        // TODO: maybe more of the parameters have to be configurable.

        let checkpointer = Checkpointer::new(&data_dir);
        let file_server = FileServer {
            // Use our special paths provider.
            paths_provider,
            // Max amount of bytes to read from a single file before switching
            // over to the next file.
            // This allows distributing the reads more or less evenly across
            // the files.
            max_read_bytes,
            // We want to use checkpointing mechanism, and resume from where we
            // left off.
            ignore_checkpoints: false,
            // Match the default behavior
            read_from,
            // We're now aware of the use cases that would require specifying
            // the starting point in time since when we should collect the logs,
            // so we just disable it. If users ask, we can expose it. There may
            // be other, more sound ways for users considering the use of this
            // option to solve their use case, so take consideration.
            ignore_before,
            // The maximum number of bytes a line can contain before being discarded. This
            // protects against malformed lines or tailing incorrect files.
            max_line_bytes,
            // Delimiter bytes that is used to read the file line-by-line
            line_delimiter: Bytes::from("\n"),
            // The directory where to keep the checkpoints.
            data_dir,
            // This value specifies not exactly the globbing, but interval
            // between the polling the files to watch from the `paths_provider`.
            glob_minimum_cooldown,
            // The shape of the log files is well-known in the Kubernetes
            // environment, so we pick the a specially crafted fingerprinter
            // for the log files.
            fingerprinter: Fingerprinter {
                strategy: FingerprintStrategy::FirstLinesChecksum {
                    // Max line length to expect during fingerprinting, see the
                    // explanation above.
                    ignored_header_bytes: 0,
                    lines: fingerprint_lines,
                },
                max_line_length: max_line_bytes,
                ignore_not_found: true,
            },
            oldest_first,
            // We do not remove the log files, `kubelet` is responsible for it.
            remove_after: None,
            // The standard emitter.
            emitter: FileSourceInternalEventsEmitter {
                include_file_metric_tag,
            },
            // A handle to the current tokio runtime
            handle: tokio::runtime::Handle::current(),
            rotate_wait,
        };

        let (file_source_tx, file_source_rx) = futures::channel::mpsc::channel::<Vec<Line>>(2);

        let checkpoints = checkpointer.view();
        let events = file_source_rx.flat_map(futures::stream::iter);
        let bytes_received = register!(BytesReceived::from(Protocol::HTTP));
        let events = events.map(move |line| {
            let byte_size = line.text.len();
            bytes_received.emit(ByteSize(byte_size));

            let mut event = create_event(
                line.text,
                &line.filename,
                ingestion_timestamp_field.as_ref(),
                log_namespace,
            );

            let file_info = annotator.annotate(&mut event, &line.filename);

            emit!(KubernetesLogsEventsReceived {
                file: &line.filename,
                byte_size: event.estimated_json_encoded_size_of(),
                pod_info: file_info.as_ref().map(|info| KubernetesLogsPodInfo {
                    name: info.pod_name.to_owned(),
                    namespace: info.pod_namespace.to_owned(),
                }),
            });

            if file_info.is_none() {
                emit!(KubernetesLogsEventAnnotationError { event: &event });
            } else {
                let namespace = file_info.as_ref().map(|info| info.pod_namespace);

                if let Some(name) = namespace {
                    let ns_info = ns_annotator.annotate(&mut event, name);

                    if ns_info.is_none() {
                        emit!(KubernetesLogsEventNamespaceAnnotationError { event: &event });
                    }
                }

                let node_info = node_annotator.annotate(&mut event, self_node_name.as_str());

                if node_info.is_none() {
                    emit!(KubernetesLogsEventNodeAnnotationError { event: &event });
                }
            }

            checkpoints.update(line.file_id, line.end_offset);
            event
        });

        let mut parser = Parser::new(log_namespace);
        let events = events.flat_map(move |event| {
            let mut buf = OutputBuffer::with_capacity(1);
            parser.transform(&mut buf, event);
            futures::stream::iter(buf.into_events())
        });

        let (events_count, _) = events.size_hint();

        let mut stream = if auto_partial_merge {
            merge_partial_events(events, log_namespace).left_stream()
        } else {
            events.right_stream()
        };

        let event_processing_loop = out.send_event_stream(&mut stream);

        let mut lifecycle = Lifecycle::new();
        {
            let (slot, shutdown) = lifecycle.add();
            let fut = util::run_file_server(file_server, file_source_tx, shutdown, checkpointer)
                .map(|result| match result {
                    Ok(FileServerShutdown) => info!(message = "File server completed gracefully."),
                    Err(error) => emit!(KubernetesLifecycleError {
                        message: "File server exited with an error.",
                        error,
                        count: events_count,
                    }),
                });
            slot.bind(Box::pin(fut));
        }
        {
            let (slot, shutdown) = lifecycle.add();
            let fut = util::complete_with_deadline_on_signal(
                event_processing_loop,
                shutdown,
                Duration::from_secs(30), // more than enough time to propagate
            )
            .map(|result| {
                match result {
                    Ok(Ok(())) => info!(message = "Event processing loop completed gracefully."),
                    Ok(Err(_)) => emit!(StreamClosedError {
                        count: events_count
                    }),
                    Err(error) => emit!(KubernetesLifecycleError {
                        error,
                        message: "Event processing loop timed out during the shutdown.",
                        count: events_count,
                    }),
                };
            });
            slot.bind(Box::pin(fut));
        }

        lifecycle.run(global_shutdown).await;
        // Stop Kubernetes object reflectors to avoid their leak on vector reload.
        for reflector in reflectors {
            reflector.abort();
        }
        info!(message = "Done.");
        Ok(())
    }
}

fn create_event(
    line: Bytes,
    file: &str,
    ingestion_timestamp_field: Option<&OwnedTargetPath>,
    log_namespace: LogNamespace,
) -> Event {
    let deserializer = BytesDeserializer;
    let mut log = deserializer.parse_single(line, log_namespace);

    log_namespace.insert_source_metadata(
        Config::NAME,
        &mut log,
        Some(LegacyKey::Overwrite(path!("file"))),
        path!("file"),
        file,
    );

    log_namespace.insert_vector_metadata(
        &mut log,
        log_schema().source_type_key(),
        path!("source_type"),
        Bytes::from(Config::NAME),
    );
    match (log_namespace, ingestion_timestamp_field) {
        // When using LogNamespace::Vector always set the ingest_timestamp.
        (LogNamespace::Vector, _) => {
            log.metadata_mut()
                .value_mut()
                .insert(path!("vector", "ingest_timestamp"), Utc::now());
        }
        // When LogNamespace::Legacy, only set when the `ingestion_timestamp_field` is configured.
        (LogNamespace::Legacy, Some(ingestion_timestamp_field)) => {
            log.try_insert(ingestion_timestamp_field, Utc::now())
        }
        // The CRI/Docker parsers handle inserting the `log_schema().timestamp_key()` value.
        (LogNamespace::Legacy, None) => (),
    };

    log.into()
}

/// This function returns the default value for `self_node_name` variable
/// as it should be at the generated config file.
fn default_self_node_name_env_template() -> String {
    format!("${{{}}}", SELF_NODE_NAME_ENV_KEY.to_owned())
}

fn default_path_inclusion() -> Vec<PathBuf> {
    vec![PathBuf::from("**/*")]
}

fn default_path_exclusion() -> Vec<PathBuf> {
    vec![PathBuf::from("**/*.gz"), PathBuf::from("**/*.tmp")]
}

const fn default_max_read_bytes() -> usize {
    2048
}

// We'd like to consume rotated pod log files first to release our file handle and let
// the space be reclaimed
const fn default_oldest_first() -> bool {
    true
}

const fn default_max_line_bytes() -> usize {
    // NOTE: The below comment documents an incorrect assumption, see
    // https://github.com/vectordotdev/vector/issues/6967
    //
    // The 16KB is the maximum size of the payload at single line for both
    // docker and CRI log formats.
    // We take a double of that to account for metadata and padding, and to
    // have a power of two rounding. Line splitting is countered at the
    // parsers, see the `partial_events_merger` logic.

    32 * 1024 // 32 KiB
}

const fn default_glob_minimum_cooldown_ms() -> Duration {
    Duration::from_millis(60_000)
}

const fn default_fingerprint_lines() -> usize {
    1
}

const fn default_delay_deletion_ms() -> Duration {
    Duration::from_millis(60_000)
}

const fn default_rotate_wait() -> Duration {
    Duration::from_secs(u64::MAX / 2)
}

// This function constructs the patterns we include for file watching, created
// from the defaults or user provided configuration.
fn prepare_include_paths(config: &Config) -> crate::Result<Vec<glob::Pattern>> {
    prepare_glob_patterns(&config.include_paths_glob_patterns, "Including")
}

// This function constructs the patterns we exclude from file watching, created
// from the defaults or user provided configuration.
fn prepare_exclude_paths(config: &Config) -> crate::Result<Vec<glob::Pattern>> {
    prepare_glob_patterns(&config.exclude_paths_glob_patterns, "Excluding")
}

// This function constructs the patterns for file watching, created
// from the defaults or user provided configuration.
fn prepare_glob_patterns(paths: &[PathBuf], op: &str) -> crate::Result<Vec<glob::Pattern>> {
    let ret = paths
        .iter()
        .map(|pattern| {
            let pattern = pattern
                .to_str()
                .ok_or("glob pattern is not a valid UTF-8 string")?;
            Ok(glob::Pattern::new(pattern)?)
        })
        .collect::<crate::Result<Vec<_>>>()?;

    info!(
        message = format!("{op} matching files."),
        ret = ?ret
            .iter()
            .map(glob::Pattern::as_str)
            .collect::<Vec<_>>()
    );

    Ok(ret)
}

// This function constructs the effective field selector to use, based on
// the specified configuration.
fn prepare_field_selector(config: &Config, self_node_name: &str) -> crate::Result<String> {
    info!(
        message = "Obtained Kubernetes Node name to collect logs for (self).",
        ?self_node_name
    );

    let field_selector = format!("spec.nodeName={}", self_node_name);

    if config.extra_field_selector.is_empty() {
        return Ok(field_selector);
    }

    Ok(format!(
        "{},{}",
        field_selector, config.extra_field_selector
    ))
}

// This function constructs the selector for a node to annotate entries with a node metadata.
fn prepare_node_selector(self_node_name: &str) -> crate::Result<String> {
    Ok(format!("metadata.name={}", self_node_name))
}

// This function constructs the effective label selector to use, based on
// the specified configuration.
fn prepare_label_selector(selector: &str) -> String {
    const BUILT_IN: &str = "vector.dev/exclude!=true";

    if selector.is_empty() {
        return BUILT_IN.to_string();
    }

    format!("{},{}", BUILT_IN, selector)
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;
    use vector_lib::lookup::{owned_value_path, OwnedTargetPath};
    use vector_lib::{config::LogNamespace, schema::Definition};
    use vrl::value::{kind::Collection, Kind};

    use crate::config::SourceConfig;

    use super::Config;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<Config>();
    }

    #[test]
    fn prepare_exclude_paths() {
        let cases = vec![
            (
                Config::default(),
                vec![
                    glob::Pattern::new("**/*.gz").unwrap(),
                    glob::Pattern::new("**/*.tmp").unwrap(),
                ],
            ),
            (
                Config {
                    exclude_paths_glob_patterns: vec![std::path::PathBuf::from("**/*.tmp")],
                    ..Default::default()
                },
                vec![glob::Pattern::new("**/*.tmp").unwrap()],
            ),
            (
                Config {
                    exclude_paths_glob_patterns: vec![
                        std::path::PathBuf::from("**/kube-system_*/**"),
                        std::path::PathBuf::from("**/*.gz"),
                        std::path::PathBuf::from("**/*.tmp"),
                    ],
                    ..Default::default()
                },
                vec![
                    glob::Pattern::new("**/kube-system_*/**").unwrap(),
                    glob::Pattern::new("**/*.gz").unwrap(),
                    glob::Pattern::new("**/*.tmp").unwrap(),
                ],
            ),
        ];

        for (input, mut expected) in cases {
            let mut output = super::prepare_exclude_paths(&input).unwrap();
            expected.sort();
            output.sort();
            assert_eq!(expected, output, "expected left, actual right");
        }
    }

    #[test]
    fn prepare_field_selector() {
        let cases = vec![
            // We're not testing `Config::default()` or empty `self_node_name`
            // as passing env vars in the concurrent tests is difficult.
            (
                Config {
                    self_node_name: "qwe".to_owned(),
                    ..Default::default()
                },
                "spec.nodeName=qwe",
            ),
            (
                Config {
                    self_node_name: "qwe".to_owned(),
                    extra_field_selector: "".to_owned(),
                    ..Default::default()
                },
                "spec.nodeName=qwe",
            ),
            (
                Config {
                    self_node_name: "qwe".to_owned(),
                    extra_field_selector: "foo=bar".to_owned(),
                    ..Default::default()
                },
                "spec.nodeName=qwe,foo=bar",
            ),
        ];

        for (input, expected) in cases {
            let output = super::prepare_field_selector(&input, "qwe").unwrap();
            assert_eq!(expected, output, "expected left, actual right");
        }
    }

    #[test]
    fn prepare_label_selector() {
        let cases = vec![
            (
                Config::default().extra_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config::default().extra_namespace_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config {
                    extra_label_selector: "".to_owned(),
                    ..Default::default()
                }
                .extra_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config {
                    extra_namespace_label_selector: "".to_owned(),
                    ..Default::default()
                }
                .extra_namespace_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config {
                    extra_label_selector: "qwe".to_owned(),
                    ..Default::default()
                }
                .extra_label_selector,
                "vector.dev/exclude!=true,qwe",
            ),
            (
                Config {
                    extra_namespace_label_selector: "qwe".to_owned(),
                    ..Default::default()
                }
                .extra_namespace_label_selector,
                "vector.dev/exclude!=true,qwe",
            ),
        ];

        for (input, expected) in cases {
            let output = super::prepare_label_selector(&input);
            assert_eq!(expected, output, "expected left, actual right");
        }
    }

    #[test]
    fn test_output_schema_definition_vector_namespace() {
        let definitions = toml::from_str::<Config>("")
            .unwrap()
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "file"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "container_id"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "container_image"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "container_name"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "namespace_labels"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "node_labels"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_annotations"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_ip"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_ips"),
                        Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_labels"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_name"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_namespace"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_node_name"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_owner"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_uid"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "stream"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "timestamp"),
                        Kind::timestamp(),
                        Some("timestamp")
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "source_type"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "ingest_timestamp"),
                        Kind::timestamp(),
                        None
                    )
                    .with_meaning(OwnedTargetPath::event_root(), "message")
            )
        )
    }

    #[test]
    fn test_output_schema_definition_legacy_namespace() {
        let definitions = toml::from_str::<Config>("")
            .unwrap()
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(
                    Kind::object(Collection::empty()),
                    [LogNamespace::Legacy]
                )
                .with_event_field(&owned_value_path!("file"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("message"),
                    Kind::bytes(),
                    Some("message")
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "container_id"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "container_image"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "container_name"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "namespace_labels"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "node_labels"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_annotations"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_ip"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_ips"),
                    Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_labels"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_name"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_namespace"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_node_name"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_owner"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_uid"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(&owned_value_path!("stream"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_event_field(
                    &owned_value_path!("source_type"),
                    Kind::bytes(),
                    None
                )
            )
        )
    }
}
