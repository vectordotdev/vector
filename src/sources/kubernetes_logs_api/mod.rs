//! API-based Kubernetes pod log collection.
//!
//! Unlike `kubernetes_logs`, which tails kubelet-managed files on the host,
//! this source streams logs through the Kubernetes `pods/log` API.

#![deny(missing_docs)]

use std::{path::PathBuf, time::Duration};

use futures::FutureExt;
use serde_with::serde_as;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalTargetPath, owned_value_path},
};
use vrl::value::{Kind, kind::Collection};

use crate::{
    config::{DataType, GenerateConfig, SourceConfig, SourceContext, SourceOutput, log_schema},
    sources,
    sources::kubernetes_logs::{
        namespace_metadata_annotator::FieldsSpec as NamespaceFieldsSpec,
        node_metadata_annotator::FieldsSpec as NodeFieldsSpec,
        pod_metadata_annotator::FieldsSpec as PodFieldsSpec,
        default_self_node_name_env_template,
    },
};

mod source;
mod stream;
mod util;

pub(super) const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);
pub(super) const EVENT_CHANNEL_SIZE: usize = 1024;

const fn default_insert_namespace_fields() -> bool {
    true
}

const fn default_delay_deletion_ms() -> Duration {
    Duration::from_millis(60_000)
}

const fn default_since_seconds() -> i64 {
    10
}

/// Configuration for the `kubernetes_logs_api` source.
#[serde_as]
#[configurable_component(source(
    "kubernetes_logs_api",
    "Collect Pod logs from the Kubernetes pods/log API."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Additional pod label selector.
    #[configurable(metadata(docs::examples = "app=my-app"))]
    extra_label_selector: String,

    /// Additional namespace label selector.
    #[configurable(metadata(docs::examples = "team=platform"))]
    extra_namespace_label_selector: String,

    /// Specifies whether or not to enrich logs with namespace fields.
    #[serde(default = "default_insert_namespace_fields")]
    insert_namespace_fields: bool,

    /// The name of the Kubernetes node Vector is running on.
    self_node_name: String,

    /// Additional pod field selector.
    #[configurable(metadata(docs::examples = "metadata.name!=pod-name-to-exclude"))]
    extra_field_selector: String,

    #[configurable(derived)]
    #[serde(alias = "annotation_fields")]
    pod_annotation_fields: PodFieldsSpec,

    #[configurable(derived)]
    namespace_annotation_fields: NamespaceFieldsSpec,

    #[configurable(derived)]
    node_annotation_fields: NodeFieldsSpec,

    /// Restrict collection to a single container name.
    #[configurable(metadata(docs::examples = "app"))]
    container: Option<String>,

    /// Lines to request on the first stream connect.
    #[serde(default)]
    tail_lines: i64,

    /// Seconds back to request on reconnect.
    #[serde(default = "default_since_seconds")]
    since_seconds: i64,

    /// Maximum number of concurrent pod log requests.
    #[serde(default)]
    max_log_requests: usize,

    /// Optional path to a readable kubeconfig file.
    #[configurable(metadata(docs::examples = "/path/to/.kube/config"))]
    kube_config_file: Option<PathBuf>,

    /// Determines if requests to the kube-apiserver can be served by a cache.
    use_apiserver_cache: bool,

    /// Delay metadata deletion after delete events.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    delay_deletion_ms: Duration,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    /// Overrides the name of the log field used to add the ingestion timestamp to each event.
    #[configurable(metadata(docs::examples = ".ingest_timestamp", docs::examples = "ingest_ts"))]
    ingestion_timestamp_field: Option<OptionalTargetPath>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            extra_label_selector: String::new(),
            extra_namespace_label_selector: String::new(),
            insert_namespace_fields: true,
            self_node_name: default_self_node_name_env_template(),
            extra_field_selector: String::new(),
            pod_annotation_fields: PodFieldsSpec::default(),
            namespace_annotation_fields: NamespaceFieldsSpec::default(),
            node_annotation_fields: NodeFieldsSpec::default(),
            container: None,
            tail_lines: 0,
            since_seconds: default_since_seconds(),
            max_log_requests: 0,
            kube_config_file: None,
            use_apiserver_cache: false,
            delay_deletion_ms: default_delay_deletion_ms(),
            log_namespace: None,
            ingestion_timestamp_field: None,
        }
    }
}

impl GenerateConfig for Config {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_logs_api")]
impl SourceConfig for Config {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let source = source::Source::new(self, &cx.globals).await?;

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
        let schema_definition = vector_lib::codecs::BytesDeserializerConfig
            .schema_definition(log_namespace)
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
