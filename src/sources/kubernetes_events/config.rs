use std::{collections::BTreeSet, path::PathBuf};

use vector_lib::{
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
    schema::Definition,
};
use vrl::value::{Kind, kind::Collection};

pub const DEFAULT_MAX_EVENT_AGE_SECS: u64 = 3600;
pub const DEFAULT_DEDUPE_RETENTION_SECS: u64 = 900;
pub const DEFAULT_WATCH_TIMEOUT_SECS: u32 = 290;
pub const DEFAULT_LEASE_NAME: &str = "vector-kubernetes-events";
pub const DEFAULT_IDENTITY_ENV_VAR: &str = "VECTOR_SELF_POD_NAME";
pub const FALLBACK_IDENTITY_ENV_VAR: &str = "HOSTNAME";
pub const POD_NAMESPACE_ENV_VAR: &str = "VECTOR_SELF_POD_NAMESPACE";
pub const SERVICE_ACCOUNT_NAMESPACE_PATH: &str =
    "/var/run/secrets/kubernetes.io/serviceaccount/namespace";
pub const DEFAULT_LEASE_DURATION_SECS: u64 = 15;
pub const DEFAULT_RENEW_DEADLINE_SECS: u64 = 10;
pub const DEFAULT_RETRY_PERIOD_SECS: u64 = 2;

/// Configuration for the `kubernetes_events` source.
#[configurable_component(source(
    "kubernetes_events",
    "Collect cluster events from the Kubernetes API."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KubernetesEventsConfig {
    /// Path to a kubeconfig file. If omitted, in-cluster configuration or the local kubeconfig is used.
    #[configurable(metadata(docs::examples = "/path/to/kubeconfig"))]
    pub kube_config_file: Option<PathBuf>,

    /// Limits the collection to the specified namespaces. If empty, all namespaces are watched.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "kube-system"))]
    pub namespaces: Vec<String>,

    /// Field selector applied to the events list/watch request.
    #[configurable(metadata(docs::examples = "regarding.kind=Pod"))]
    pub field_selector: Option<String>,

    /// Label selector applied to the events list/watch request.
    #[configurable(metadata(docs::examples = "type=Warning"))]
    pub label_selector: Option<String>,

    /// Restricts the source to the specified event types (for example, `Warning`). Empty means all types.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "Warning"))]
    pub include_types: Vec<String>,

    /// Restricts the source to the specified reasons. Empty means all reasons.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "FailedScheduling"))]
    pub include_reasons: Vec<String>,

    /// Restricts the source to the specified involved object kinds. Empty means all kinds.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "Pod"))]
    pub include_involved_object_kinds: Vec<String>,

    /// Maximum age of an event to forward.
    #[serde(default = "default_max_event_age_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Maximum Event Age"))]
    pub max_event_age_seconds: u64,

    /// Retention window for deduplication state.
    #[serde(default = "default_dedupe_retention_seconds")]
    #[configurable(metadata(
        docs::type_unit = "seconds",
        docs::human_name = "Deduplication Retention"
    ))]
    pub dedupe_retention_seconds: u64,

    /// Timeout applied to the Kubernetes watch call.
    #[serde(default = "default_watch_timeout_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Watch Timeout"))]
    pub watch_timeout_seconds: u32,

    /// When enabled, the previous version of the event is included in the emitted payload on updates.
    #[serde(default)]
    pub include_previous_event: bool,

    /// Lease-based leader election settings for running multiple replicas safely.
    #[serde(default)]
    pub leader_election: KubernetesEventsLeaderElectionConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

/// Configuration for Kubernetes Lease-based leader election.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KubernetesEventsLeaderElectionConfig {
    /// Enables Lease-based leader election.
    #[serde(default)]
    pub enabled: bool,

    /// Name of the Kubernetes Lease object used for coordination.
    #[serde(default = "default_lease_name")]
    #[configurable(metadata(docs::examples = "vector-kubernetes-events"))]
    pub lease_name: String,

    /// Namespace containing the Kubernetes Lease object.
    ///
    /// If omitted, Vector uses `VECTOR_SELF_POD_NAMESPACE`, then the in-cluster service account
    /// namespace file, then `default`.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "observability"))]
    pub lease_namespace: Option<String>,

    /// Environment variable containing this replica's leader election identity.
    ///
    /// If this variable is not set, Vector falls back to `HOSTNAME`.
    #[serde(default = "default_identity_env_var")]
    #[configurable(metadata(docs::examples = "VECTOR_SELF_POD_NAME"))]
    pub identity_env_var: String,

    /// Lease duration.
    #[serde(default = "default_lease_duration_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Lease Duration"))]
    pub lease_duration_seconds: u64,

    /// Maximum time this replica will continue as leader without a successful renewal.
    #[serde(default = "default_renew_deadline_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Renew Deadline"))]
    pub renew_deadline_seconds: u64,

    /// Time between leader election acquire and renew attempts.
    #[serde(default = "default_retry_period_seconds")]
    #[configurable(metadata(docs::type_unit = "seconds", docs::human_name = "Retry Period"))]
    pub retry_period_seconds: u64,
}

impl Default for KubernetesEventsLeaderElectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            lease_name: DEFAULT_LEASE_NAME.to_string(),
            lease_namespace: None,
            identity_env_var: DEFAULT_IDENTITY_ENV_VAR.to_string(),
            lease_duration_seconds: DEFAULT_LEASE_DURATION_SECS,
            renew_deadline_seconds: DEFAULT_RENEW_DEADLINE_SECS,
            retry_period_seconds: DEFAULT_RETRY_PERIOD_SECS,
        }
    }
}

impl Default for KubernetesEventsConfig {
    fn default() -> Self {
        Self {
            kube_config_file: None,
            namespaces: Vec::new(),
            field_selector: None,
            label_selector: None,
            include_types: Vec::new(),
            include_reasons: Vec::new(),
            include_involved_object_kinds: Vec::new(),
            max_event_age_seconds: DEFAULT_MAX_EVENT_AGE_SECS,
            dedupe_retention_seconds: DEFAULT_DEDUPE_RETENTION_SECS,
            watch_timeout_seconds: DEFAULT_WATCH_TIMEOUT_SECS,
            include_previous_event: false,
            leader_election: KubernetesEventsLeaderElectionConfig::default(),
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(KubernetesEventsConfig);

const fn default_max_event_age_seconds() -> u64 {
    DEFAULT_MAX_EVENT_AGE_SECS
}

const fn default_dedupe_retention_seconds() -> u64 {
    DEFAULT_DEDUPE_RETENTION_SECS
}

const fn default_watch_timeout_seconds() -> u32 {
    DEFAULT_WATCH_TIMEOUT_SECS
}

fn default_lease_name() -> String {
    DEFAULT_LEASE_NAME.to_string()
}

fn default_identity_env_var() -> String {
    DEFAULT_IDENTITY_ENV_VAR.to_string()
}

const fn default_lease_duration_seconds() -> u64 {
    DEFAULT_LEASE_DURATION_SECS
}

const fn default_renew_deadline_seconds() -> u64 {
    DEFAULT_RENEW_DEADLINE_SECS
}

const fn default_retry_period_seconds() -> u64 {
    DEFAULT_RETRY_PERIOD_SECS
}

pub fn schema_definition(log_namespace: LogNamespace) -> Definition {
    let mut namespaces = BTreeSet::new();
    namespaces.insert(log_namespace);

    Definition::new_with_default_metadata(Kind::object(Collection::any()), namespaces)
        .with_standard_vector_source_metadata()
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("namespace"))),
            &owned_value_path!("namespace"),
            Kind::bytes().or_undefined(),
            Some("namespace"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("verb"))),
            &owned_value_path!("verb"),
            Kind::bytes(),
            Some("verb"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("event_uid"))),
            &owned_value_path!("event_uid"),
            Kind::bytes(),
            Some("event_uid"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("reason"))),
            &owned_value_path!("reason"),
            Kind::bytes().or_undefined(),
            Some("reason"),
        )
        .with_source_metadata(
            KubernetesEventsConfig::NAME,
            Some(LegacyKey::InsertIfEmpty(owned_value_path!("type"))),
            &owned_value_path!("type"),
            Kind::bytes().or_undefined(),
            Some("event_type"),
        )
}
