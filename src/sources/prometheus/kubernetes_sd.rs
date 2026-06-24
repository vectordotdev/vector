//! `prometheus_kubernetes_sd` source.
//!
//! Auto-discovers and scrapes Prometheus metrics from Kubernetes Pods using
//! Prometheus-compatible `prometheus.io/*` annotations. Equivalent in spirit to
//! Prometheus' `kubernetes_sd_configs` with `role: pod`.
//!
//! The source watches Pods via the Kubernetes API, applies annotation-based
//! discovery rules to derive a set of scrape targets, and periodically scrapes
//! them concurrently. The list of targets is refreshed on every scrape tick by
//! reading the latest reflector state, so discovery is fully dynamic and does
//! not require a Vector config reload when Pods come and go.

#![deny(missing_docs)]

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use futures::{StreamExt, stream};
use http::{Uri, response::Parts};
use http_1::{HeaderName, HeaderValue};
use hyper::{Body, Request};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Client, Config as ClientConfig,
    api::Api,
    config::{self, KubeConfigOptions},
    runtime::{WatchStreamExt, reflector, watcher},
};
use serde_with::serde_as;
use snafu::Snafu;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    config::LogNamespace,
    configurable::configurable_component,
    event::{Event, Metric},
    json_size::JsonSize,
    shutdown::ShutdownSignal,
};

use super::parser;
use crate::{
    SourceSender,
    built_info::{PKG_NAME, PKG_VERSION},
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    http::{Auth, HttpClient},
    internal_events::{
        EndpointBytesReceived, HttpClientEventsReceived, HttpClientHttpError,
        HttpClientHttpResponseError, PrometheusKubernetesSdAnnotationParseError,
        PrometheusKubernetesSdTargetsDiscovered, PrometheusParseError, StreamClosedError,
    },
    kubernetes::{custom_reflector, meta_cache::MetaCache},
    sources::util::http_client::{default_interval, default_timeout, warn_if_interval_too_low},
    tls::{TlsConfig, TlsSettings},
};

/// Env var consulted for the current node name when `use_self_node_only` is set.
const SELF_NODE_NAME_ENV_KEY: &str = "VECTOR_SELF_NODE_NAME";

/// Default delay between observing a Pod deletion and removing it from the
/// reflector store; matches `kubernetes_logs` and gives in-flight scrapes time
/// to settle.
const fn default_delay_deletion_ms() -> u64 {
    60_000
}

const fn default_scheme() -> Scheme {
    Scheme::Http
}

fn default_path() -> String {
    "/metrics".to_string()
}

fn default_annotation_prefix() -> String {
    "prometheus.io".to_string()
}

fn default_instance_tag() -> Option<String> {
    Some("instance".to_string())
}

fn default_endpoint_tag() -> Option<String> {
    Some("endpoint".to_string())
}

/// Discovery role.
///
/// Only `pod` is currently supported.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Discover targets from Pods.
    #[default]
    Pod,
}

/// Default scheme used when a Pod does not set `prometheus.io/scheme`.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// Plain HTTP.
    #[default]
    Http,
    /// HTTPS.
    Https,
}

impl Scheme {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "http" => Some(Self::Http),
            "https" => Some(Self::Https),
            _ => None,
        }
    }
}

/// Configuration for the `prometheus_kubernetes_sd` source.
#[serde_as]
#[configurable_component(source(
    "prometheus_kubernetes_sd",
    "Auto-discover and scrape Prometheus metrics from Kubernetes Pods using prometheus.io/* annotations."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct PrometheusKubernetesSdConfig {
    /// Discovery role. Only `pod` is supported in this release.
    role: Role,

    /// Interval between scrape ticks.
    ///
    /// On each tick, the current set of discovered targets is snapshotted and
    /// scraped concurrently.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    interval: Duration,

    /// Per-target HTTP request timeout.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Timeout"))]
    timeout: Duration,

    /// Annotation prefix to read scrape configuration from.
    ///
    /// Defaults to `prometheus.io`, matching the de-facto Prometheus convention.
    /// The source reads `<prefix>/scrape`, `<prefix>/port`, `<prefix>/path`,
    /// `<prefix>/scheme`, and `<prefix>/param_<name>` from Pod annotations.
    #[configurable(metadata(docs::examples = "prometheus.io"))]
    annotation_prefix: String,

    /// Restrict discovery to Pods on the current node.
    ///
    /// When enabled, the source filters Pods by `spec.nodeName=<node>` where
    /// `<node>` is read from the `VECTOR_SELF_NODE_NAME` environment variable
    /// (or the `self_node_name` option). Use this when deploying Vector as a
    /// DaemonSet.
    use_self_node_only: bool,

    /// Override for the node name used by `use_self_node_only`.
    ///
    /// If unset and `use_self_node_only` is `true`, the `VECTOR_SELF_NODE_NAME`
    /// environment variable is read instead.
    #[configurable(metadata(docs::examples = "node-01"))]
    self_node_name: Option<String>,

    /// Additional field selector merged with the built-in filter.
    #[configurable(metadata(docs::examples = "metadata.namespace=monitoring"))]
    extra_field_selector: String,

    /// Additional label selector merged with the built-in `vector.dev/exclude!=true` filter.
    #[configurable(metadata(docs::examples = "tier=frontend"))]
    extra_label_selector: String,

    /// Restrict discovery to specific namespaces.
    ///
    /// When non-empty, only Pods in the listed namespaces are watched. When
    /// empty, all namespaces are watched (requires cluster-wide RBAC).
    #[configurable(metadata(docs::examples = "monitoring"))]
    namespaces: Vec<String>,

    /// Path to a kubeconfig file.
    ///
    /// When unset, the source falls back to the local kubeconfig followed by
    /// in-cluster service-account credentials.
    kube_config_file: Option<PathBuf>,

    /// Delay between observing a Pod deletion event and removing the Pod from
    /// the discovery store, in milliseconds.
    delay_deletion_ms: u64,

    /// Default scheme when `<prefix>/scheme` is not set on a Pod.
    default_scheme: Scheme,

    /// Default scrape path when `<prefix>/path` is not set on a Pod.
    #[configurable(metadata(docs::examples = "/metrics"))]
    default_path: String,

    /// Allowlist of Pod labels to add as metric tags.
    ///
    /// Each entry is the label key; tags are emitted with the same key on each
    /// metric. Default is empty to avoid cardinality blow-ups.
    #[configurable(metadata(docs::examples = "app"))]
    #[configurable(metadata(docs::examples = "version"))]
    pod_label_tags: Vec<String>,

    /// Allowlist of Pod annotations to add as metric tags.
    #[configurable(metadata(docs::examples = "owner"))]
    pod_annotation_tags: Vec<String>,

    /// Tag name under which to record the scraped instance (host:port).
    ///
    /// Mirrors the `instance_tag` option of `prometheus_scrape`. Set to `null`
    /// to disable.
    instance_tag: Option<String>,

    /// Tag name under which to record the full scrape endpoint URL.
    ///
    /// Mirrors the `endpoint_tag` option of `prometheus_scrape`. Set to `null`
    /// to disable.
    endpoint_tag: Option<String>,

    /// Honor labels emitted by the scraped target.
    ///
    /// Mirrors the `honor_labels` option of `prometheus_scrape` and the
    /// equivalent Prometheus setting.
    honor_labels: bool,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    auth: Option<Auth>,
}

impl Default for PrometheusKubernetesSdConfig {
    fn default() -> Self {
        Self {
            role: Role::default(),
            interval: default_interval(),
            timeout: default_timeout(),
            annotation_prefix: default_annotation_prefix(),
            use_self_node_only: false,
            self_node_name: None,
            extra_field_selector: String::new(),
            extra_label_selector: String::new(),
            namespaces: Vec::new(),
            kube_config_file: None,
            delay_deletion_ms: default_delay_deletion_ms(),
            default_scheme: default_scheme(),
            default_path: default_path(),
            pod_label_tags: Vec::new(),
            pod_annotation_tags: Vec::new(),
            instance_tag: default_instance_tag(),
            endpoint_tag: default_endpoint_tag(),
            honor_labels: false,
            tls: None,
            auth: None,
        }
    }
}

impl GenerateConfig for PrometheusKubernetesSdConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display(
        "self_node_name config value or {SELF_NODE_NAME_ENV_KEY} env var must be set when use_self_node_only is true"
    ))]
    SelfNodeNameMissing,
    #[snafu(display("failed to read kubeconfig: {source}"))]
    Kubeconfig {
        source: kube::config::KubeconfigError,
    },
    #[snafu(display("failed to infer Kubernetes client config: {source}"))]
    InferConfig {
        source: kube::config::InferConfigError,
    },
    #[snafu(display("failed to build Kubernetes client: {source}"))]
    Client { source: kube::Error },
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_kubernetes_sd")]
impl SourceConfig for PrometheusKubernetesSdConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        warn_if_interval_too_low(self.timeout, self.interval);

        let client = build_kube_client(self.kube_config_file.as_ref()).await?;
        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        // Resolve self node name if filtering by node.
        let self_node_name = if self.use_self_node_only {
            let resolved = match self.self_node_name.clone() {
                Some(n) => n,
                None => std::env::var(SELF_NODE_NAME_ENV_KEY)
                    .map_err(|_| Box::new(BuildError::SelfNodeNameMissing) as crate::Error)?,
            };
            Some(resolved)
        } else {
            None
        };

        let field_selector =
            build_field_selector(self_node_name.as_deref(), &self.extra_field_selector);
        let label_selector = build_label_selector(&self.extra_label_selector);

        let parser_cfg = AnnotationParserConfig {
            prefix: self.annotation_prefix.clone(),
            default_scheme: self.default_scheme,
            default_path: self.default_path.clone(),
            pod_label_tags: self.pod_label_tags.clone(),
            pod_annotation_tags: self.pod_annotation_tags.clone(),
        };

        let scrape_cfg = ScrapeConfig {
            interval: self.interval,
            timeout: self.timeout,
            instance_tag: self.instance_tag.clone(),
            endpoint_tag: self.endpoint_tag.clone(),
            honor_labels: self.honor_labels,
            auth: self.auth.clone(),
        };

        let delay_deletion = Duration::from_millis(self.delay_deletion_ms);
        let namespaces = self.namespaces.clone();
        let proxy = cx.proxy.clone();

        Ok(Box::pin(run(
            client,
            tls,
            proxy,
            namespaces,
            field_selector,
            label_selector,
            delay_deletion,
            parser_cfg,
            scrape_cfg,
            cx.out,
            cx.shutdown,
        )))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

/// Build a Kubernetes client mirroring the pattern used by `kubernetes_logs`.
async fn build_kube_client(kube_config_file: Option<&PathBuf>) -> crate::Result<Client> {
    let mut client_config = match kube_config_file {
        Some(kc) => ClientConfig::from_custom_kubeconfig(
            config::Kubeconfig::read_from(kc)
                .map_err(|source| BuildError::Kubeconfig { source })?,
            &KubeConfigOptions::default(),
        )
        .await
        .map_err(|source| BuildError::Kubeconfig { source })?,
        None => ClientConfig::infer()
            .await
            .map_err(|source| BuildError::InferConfig { source })?,
    };
    if let Ok(user_agent) = HeaderValue::from_str(&format!("{PKG_NAME}/{PKG_VERSION}")) {
        client_config
            .headers
            .push((HeaderName::from_static("user-agent"), user_agent));
    }
    Client::try_from(client_config)
        .map_err(|source| Box::new(BuildError::Client { source }) as crate::Error)
}

fn build_field_selector(self_node_name: Option<&str>, extra: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(node) = self_node_name {
        parts.push(format!("spec.nodeName={node}"));
    }
    if !extra.is_empty() {
        parts.push(extra.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(","))
    }
}

fn build_label_selector(extra: &str) -> String {
    const BUILT_IN: &str = "vector.dev/exclude!=true";
    if extra.is_empty() {
        BUILT_IN.to_string()
    } else {
        format!("{BUILT_IN},{extra}")
    }
}

/// Per-target scrape parameters that don't depend on the discovered Pod.
#[derive(Clone)]
struct ScrapeConfig {
    interval: Duration,
    timeout: Duration,
    instance_tag: Option<String>,
    endpoint_tag: Option<String>,
    honor_labels: bool,
    auth: Option<Auth>,
}

/// Top-level scrape loop. Owns the reflector spawn and the periodic scraper.
#[allow(clippy::too_many_arguments)]
async fn run(
    client: Client,
    tls: TlsSettings,
    proxy: vector_lib::config::proxy::ProxyConfig,
    namespaces: Vec<String>,
    field_selector: Option<String>,
    label_selector: String,
    delay_deletion: Duration,
    parser_cfg: AnnotationParserConfig,
    scrape_cfg: ScrapeConfig,
    mut out: SourceSender,
    shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let store_w = reflector::store::Writer::<Pod>::default();
    let store_r = store_w.as_reader();

    // We always run a single cluster-wide watcher feeding one shared store.
    //
    // When `namespaces` is non-empty, we apply the restriction in two ways:
    //   * If exactly one namespace is configured, it is added as a server-side
    //     `metadata.namespace=<ns>` field selector to minimize wire traffic.
    //   * In all multi-namespace cases, we additionally filter client-side in
    //     [`collect_targets`] using the `allowed_namespaces` set. Kubernetes
    //     field selectors do not support `in` semantics, so client-side
    //     filtering is the most reliable option.
    let mut field_parts = field_selector.clone().map(|s| vec![s]).unwrap_or_default();
    if namespaces.len() == 1 {
        field_parts.push(format!("metadata.namespace={}", namespaces[0]));
    }
    let watcher_cfg = watcher::Config {
        field_selector: if field_parts.is_empty() {
            None
        } else {
            Some(field_parts.join(","))
        },
        label_selector: Some(label_selector.clone()),
        ..Default::default()
    };

    let api = Api::<Pod>::all(client.clone());
    let stream = watcher(api, watcher_cfg).backoff(watcher::DefaultBackoff::default());
    let reflector_handles = vec![crate::spawn_in_current_span(custom_reflector(
        store_w,
        MetaCache::new(),
        stream,
        delay_deletion,
    ))];

    let http_client = match HttpClient::new(tls, &proxy) {
        Ok(c) => c,
        Err(error) => {
            error!(message = "Failed to build HTTP client.", %error);
            for handle in reflector_handles {
                handle.abort();
            }
            return Err(());
        }
    };

    let parser_cfg = Arc::new(parser_cfg);
    let scrape_cfg = Arc::new(scrape_cfg);
    let allowed_namespaces: HashSet<String> = namespaces.iter().cloned().collect();

    let mut interval_stream =
        IntervalStream::new(tokio::time::interval(scrape_cfg.interval)).take_until(shutdown);

    let result = loop {
        if interval_stream.next().await.is_none() {
            break Ok(());
        }

        let targets = collect_targets(&store_r, &parser_cfg, &allowed_namespaces);
        emit!(PrometheusKubernetesSdTargetsDiscovered {
            count: targets.len()
        });

        let scrape_cfg_outer = Arc::clone(&scrape_cfg);
        let http_client = http_client.clone();
        let mut events_stream = stream::iter(targets)
            .map(move |target| {
                let http_client = http_client.clone();
                let scrape_cfg = Arc::clone(&scrape_cfg_outer);
                async move { scrape_target(&http_client, &scrape_cfg, target).await }
            })
            .buffer_unordered(usize::MAX)
            .flat_map(stream::iter);

        if out.send_event_stream(&mut events_stream).await.is_err() {
            emit!(StreamClosedError { count: 0 });
            break Err(());
        }
    };

    for handle in reflector_handles {
        handle.abort();
    }
    result
}

fn collect_targets(
    store: &reflector::store::Store<Pod>,
    parser_cfg: &AnnotationParserConfig,
    allowed_namespaces: &HashSet<String>,
) -> Vec<Target> {
    let mut out = Vec::new();
    for pod in store.state() {
        if !allowed_namespaces.is_empty() {
            match pod.metadata.namespace.as_deref() {
                Some(ns) if allowed_namespaces.contains(ns) => {}
                _ => continue,
            }
        }
        match extract_targets(&pod, parser_cfg) {
            Ok(mut targets) => out.append(&mut targets),
            Err(error) => {
                emit!(PrometheusKubernetesSdAnnotationParseError {
                    pod: pod.metadata.name.as_deref().unwrap_or("<unknown>"),
                    namespace: pod.metadata.namespace.as_deref().unwrap_or(""),
                    error: &error.to_string(),
                });
            }
        }
    }
    out
}

/// A single scrape target derived from a Pod annotation set.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    uri: Uri,
    instance: String,
    namespace: String,
    pod_name: String,
    pod_uid: String,
    node_name: Option<String>,
    container_name: Option<String>,
    /// Additional metric tags to apply (pod labels/annotations allowlist).
    extra_tags: BTreeMap<String, String>,
}

/// Annotation-parser configuration shared across ticks.
#[derive(Clone, Debug)]
struct AnnotationParserConfig {
    prefix: String,
    default_scheme: Scheme,
    default_path: String,
    pod_label_tags: Vec<String>,
    pod_annotation_tags: Vec<String>,
}

#[derive(Debug, Snafu)]
enum AnnotationError {
    #[snafu(display("invalid value for {key}: {value}"))]
    InvalidValue { key: String, value: String },
    #[snafu(display("pod has no IP address yet"))]
    NoPodIp,
    #[snafu(display("could not resolve port: {message}"))]
    PortResolution { message: String },
    #[snafu(display("failed to build URI: {source}"))]
    UriBuild { source: http::Error },
}

/// Extract zero or more scrape targets from a Pod.
///
/// Returns an empty vector when the Pod does not opt-in via
/// `<prefix>/scrape=true`. Returns an error only for malformed annotations.
fn extract_targets(
    pod: &Pod,
    cfg: &AnnotationParserConfig,
) -> Result<Vec<Target>, AnnotationError> {
    let annotations = pod
        .metadata
        .annotations
        .as_ref()
        .map(|m| m.iter().collect::<BTreeMap<_, _>>())
        .unwrap_or_default();

    let scrape_key = format!("{}/scrape", cfg.prefix);
    let scrape = annotations
        .get(&scrape_key)
        .map(|v| v.as_str())
        .unwrap_or("false");
    if !is_truthy(scrape) {
        return Ok(Vec::new());
    }

    let path_key = format!("{}/path", cfg.prefix);
    let path = annotations
        .get(&path_key)
        .map(|s| s.as_str())
        .unwrap_or(cfg.default_path.as_str())
        .to_string();

    let scheme_key = format!("{}/scheme", cfg.prefix);
    let scheme = match annotations.get(&scheme_key) {
        Some(v) => Scheme::parse(v.as_str()).ok_or_else(|| AnnotationError::InvalidValue {
            key: scheme_key.clone(),
            value: v.to_string(),
        })?,
        None => cfg.default_scheme,
    };

    let port_key = format!("{}/port", cfg.prefix);
    let port_annotation = annotations.get(&port_key).map(|s| s.to_string());

    // Build query string from `<prefix>/param_*` annotations.
    let param_prefix = format!("{}/param_", cfg.prefix);
    let mut query_pairs: Vec<(String, String)> = annotations
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix(param_prefix.as_str())
                .map(|name| (name.to_string(), v.to_string()))
        })
        .collect();
    query_pairs.sort();

    let pod_ip = pod
        .status
        .as_ref()
        .and_then(|s| s.pod_ip.clone())
        .ok_or(AnnotationError::NoPodIp)?;
    let pod_name = pod
        .metadata
        .name
        .clone()
        .unwrap_or_else(|| "<unknown>".to_string());
    let pod_uid = pod.metadata.uid.clone().unwrap_or_else(|| pod_name.clone());
    let namespace = pod.metadata.namespace.clone().unwrap_or_default();
    let node_name = pod.spec.as_ref().and_then(|s| s.node_name.clone());

    let ports = resolve_ports(pod, port_annotation.as_deref()).map_err(|message| {
        AnnotationError::PortResolution {
            message: message.to_string(),
        }
    })?;
    if ports.is_empty() {
        return Ok(Vec::new());
    }

    let extra_tags = build_extra_tags(pod, cfg);

    let mut targets = Vec::with_capacity(ports.len());
    for port_info in ports {
        let uri = build_target_uri(scheme, &pod_ip, port_info.port, &path, &query_pairs)?;
        let instance = format!("{pod_ip}:{}", port_info.port);
        targets.push(Target {
            uri,
            instance,
            namespace: namespace.clone(),
            pod_name: pod_name.clone(),
            pod_uid: pod_uid.clone(),
            node_name: node_name.clone(),
            container_name: port_info.container,
            extra_tags: extra_tags.clone(),
        });
    }
    Ok(targets)
}

fn is_truthy(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

/// Resolved port information: numeric port and the owning container name.
#[derive(Clone, Debug)]
struct ResolvedPort {
    port: u16,
    container: Option<String>,
}

/// Resolve scrape ports from a Pod and an optional `prometheus.io/port`
/// annotation value (which may be a numeric port or a container port name).
fn resolve_ports(pod: &Pod, port_annotation: Option<&str>) -> Result<Vec<ResolvedPort>, String> {
    let spec = match pod.spec.as_ref() {
        Some(s) => s,
        None => return Ok(Vec::new()),
    };

    // Pre-compute all ports with their container names for lookup.
    let mut all_ports: Vec<(Option<String>, k8s_openapi::api::core::v1::ContainerPort)> =
        Vec::new();
    for container in &spec.containers {
        if let Some(ports) = &container.ports {
            for p in ports {
                all_ports.push((Some(container.name.clone()), p.clone()));
            }
        }
    }

    if let Some(annotation) = port_annotation {
        let annotation = annotation.trim();
        if annotation.is_empty() {
            return Err("prometheus.io/port annotation is empty".to_string());
        }
        // Try numeric first.
        if let Ok(port) = annotation.parse::<u16>() {
            // Optionally find owning container by matching containerPort.
            let container = all_ports
                .iter()
                .find(|(_, p)| p.container_port == port as i32)
                .and_then(|(name, _)| name.clone());
            return Ok(vec![ResolvedPort { port, container }]);
        }
        // Fall back to a named container port.
        if let Some((container, p)) = all_ports
            .iter()
            .find(|(_, p)| p.name.as_deref() == Some(annotation))
        {
            let port = u16::try_from(p.container_port)
                .map_err(|_| format!("port {} is out of range", p.container_port))?;
            return Ok(vec![ResolvedPort {
                port,
                container: container.clone(),
            }]);
        }
        return Err(format!(
            "no port named or numbered '{annotation}' found on pod"
        ));
    }

    // No annotation: prefer named "metrics"-like ports, else the first
    // declared port.
    let preferred: Vec<ResolvedPort> = all_ports
        .iter()
        .filter(|(_, p)| matches!(p.name.as_deref(), Some(name) if is_metrics_port_name(name)))
        .filter_map(|(container, p)| {
            u16::try_from(p.container_port)
                .ok()
                .map(|port| ResolvedPort {
                    port,
                    container: container.clone(),
                })
        })
        .collect();
    if !preferred.is_empty() {
        return Ok(preferred);
    }

    if let Some((container, p)) = all_ports.first()
        && let Ok(port) = u16::try_from(p.container_port)
    {
        return Ok(vec![ResolvedPort {
            port,
            container: container.clone(),
        }]);
    }
    Ok(Vec::new())
}

fn is_metrics_port_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "metrics" || lower == "prometheus" || lower == "http-metrics"
}

fn build_extra_tags(pod: &Pod, cfg: &AnnotationParserConfig) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    if let Some(labels) = pod.metadata.labels.as_ref() {
        for key in &cfg.pod_label_tags {
            if let Some(v) = labels.get(key) {
                tags.insert(key.clone(), v.clone());
            }
        }
    }
    if let Some(anns) = pod.metadata.annotations.as_ref() {
        for key in &cfg.pod_annotation_tags {
            if let Some(v) = anns.get(key) {
                tags.insert(key.clone(), v.clone());
            }
        }
    }
    tags
}

fn build_target_uri(
    scheme: Scheme,
    host: &str,
    port: u16,
    path: &str,
    query_pairs: &[(String, String)],
) -> Result<Uri, AnnotationError> {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let path_and_query = if query_pairs.is_empty() {
        path
    } else {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in query_pairs {
            ser.append_pair(k, v);
        }
        format!("{path}?{}", ser.finish())
    };
    Uri::builder()
        .scheme(scheme.as_str())
        .authority(format!("{host}:{port}"))
        .path_and_query(path_and_query)
        .build()
        .map_err(|source| AnnotationError::UriBuild { source })
}

/// Perform a single scrape and return the resulting events. Emits all the
/// usual HTTP-client internal events on the way.
async fn scrape_target(client: &HttpClient, cfg: &ScrapeConfig, target: Target) -> Vec<Event> {
    let endpoint = target.uri.to_string();
    let mut request = match Request::get(&target.uri).body(Body::empty()) {
        Ok(r) => r,
        Err(error) => {
            emit!(HttpClientHttpError {
                error: error.to_string().into(),
                url: endpoint,
            });
            return Vec::new();
        }
    };
    request.headers_mut().insert(
        http::header::ACCEPT,
        http::HeaderValue::from_static("text/plain"),
    );
    if let Some(auth) = &cfg.auth {
        auth.apply(&mut request);
    }

    let response = match tokio::time::timeout(cfg.timeout, client.send(request)).await {
        Ok(Ok(r)) => r,
        Ok(Err(error)) => {
            emit!(HttpClientHttpError {
                error: error.into(),
                url: endpoint,
            });
            return Vec::new();
        }
        Err(_) => {
            emit!(HttpClientHttpError {
                error: format!(
                    "Timeout error: request exceeded {}s",
                    cfg.timeout.as_secs_f64()
                )
                .into(),
                url: endpoint,
            });
            return Vec::new();
        }
    };

    let (parts, body) = response.into_parts();
    let body: Bytes = match http_body::Body::collect(body).await {
        Ok(b) => b.to_bytes(),
        Err(error) => {
            emit!(HttpClientHttpError {
                error: error.into(),
                url: endpoint,
            });
            return Vec::new();
        }
    };
    emit!(EndpointBytesReceived {
        byte_size: body.len(),
        protocol: "http",
        endpoint: endpoint.as_str(),
    });

    if parts.status != hyper::StatusCode::OK {
        on_http_response_error(&target.uri, &parts);
        emit!(HttpClientHttpResponseError {
            code: parts.status,
            url: endpoint,
        });
        return Vec::new();
    }

    let body_str = String::from_utf8_lossy(&body);
    let mut events = match parser::parse_text(&body_str) {
        Ok(events) => events,
        Err(error) => {
            emit!(PrometheusParseError {
                error,
                url: target.uri.clone(),
                body: body_str,
            });
            return Vec::new();
        }
    };

    let byte_size = if events.is_empty() {
        JsonSize::zero()
    } else {
        events.estimated_json_encoded_size_of()
    };
    emit!(HttpClientEventsReceived {
        byte_size,
        count: events.len(),
        url: endpoint,
    });

    enrich_events(&mut events, &target, cfg);
    events
}

fn on_http_response_error(url: &Uri, header: &Parts) {
    if header.status == hyper::StatusCode::NOT_FOUND && url.path() == "/" {
        warn!(
            message = "No path is set on the endpoint and we got a 404, did you mean to use /metrics?",
            endpoint = %url,
        );
    }
}

fn enrich_events(events: &mut [Event], target: &Target, cfg: &ScrapeConfig) {
    for event in events.iter_mut() {
        let metric: &mut Metric = event.as_mut_metric();
        if let Some(tag) = &cfg.instance_tag {
            super::merge_honor_label_tag(metric, tag, &target.instance, cfg.honor_labels);
        }
        if let Some(tag) = &cfg.endpoint_tag {
            super::merge_honor_label_tag(metric, tag, &target.uri.to_string(), cfg.honor_labels);
        }
        // Base discovery tags.
        super::merge_honor_label_tag(metric, "namespace", &target.namespace, cfg.honor_labels);
        super::merge_honor_label_tag(metric, "pod", &target.pod_name, cfg.honor_labels);
        if let Some(node) = &target.node_name {
            super::merge_honor_label_tag(metric, "node", node, cfg.honor_labels);
        }
        if let Some(container) = &target.container_name {
            super::merge_honor_label_tag(metric, "container", container, cfg.honor_labels);
        }
        for (k, v) in &target.extra_tags {
            super::merge_honor_label_tag(metric, k, v, cfg.honor_labels);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{Container, ContainerPort, PodSpec, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use std::collections::BTreeMap;

    fn make_pod(
        name: &str,
        namespace: &str,
        annotations: &[(&str, &str)],
        labels: &[(&str, &str)],
        ports: Vec<ContainerPort>,
        pod_ip: Option<&str>,
        node: Option<&str>,
    ) -> Pod {
        let mut anns = BTreeMap::new();
        for (k, v) in annotations {
            anns.insert((*k).to_string(), (*v).to_string());
        }
        let mut lbls = BTreeMap::new();
        for (k, v) in labels {
            lbls.insert((*k).to_string(), (*v).to_string());
        }
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
                uid: Some(format!("uid-{name}")),
                annotations: if anns.is_empty() { None } else { Some(anns) },
                labels: if lbls.is_empty() { None } else { Some(lbls) },
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "app".to_string(),
                    ports: if ports.is_empty() { None } else { Some(ports) },
                    ..Default::default()
                }],
                node_name: node.map(String::from),
                ..Default::default()
            }),
            status: Some(PodStatus {
                pod_ip: pod_ip.map(String::from),
                ..Default::default()
            }),
        }
    }

    fn default_cfg() -> AnnotationParserConfig {
        AnnotationParserConfig {
            prefix: "prometheus.io".to_string(),
            default_scheme: Scheme::Http,
            default_path: "/metrics".to_string(),
            pod_label_tags: vec![],
            pod_annotation_tags: vec![],
        }
    }

    #[test]
    fn generate_config_smoke() {
        crate::test_util::test_generate_config::<PrometheusKubernetesSdConfig>();
    }

    #[test]
    fn no_scrape_annotation_yields_no_targets() {
        let pod = make_pod(
            "p",
            "ns",
            &[],
            &[],
            vec![ContainerPort {
                container_port: 9100,
                ..Default::default()
            }],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        assert!(extract_targets(&pod, &cfg).unwrap().is_empty());
    }

    #[test]
    fn scrape_false_yields_no_targets() {
        let pod = make_pod(
            "p",
            "ns",
            &[("prometheus.io/scrape", "false")],
            &[],
            vec![ContainerPort {
                container_port: 9100,
                ..Default::default()
            }],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        assert!(extract_targets(&pod, &cfg).unwrap().is_empty());
    }

    #[test]
    fn numeric_port_annotation_resolves_container() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/port", "9100"),
            ],
            &[],
            vec![ContainerPort {
                container_port: 9100,
                name: Some("metrics".to_string()),
                ..Default::default()
            }],
            Some("10.0.0.1"),
            Some("node-a"),
        );
        let cfg = default_cfg();
        let targets = extract_targets(&pod, &cfg).unwrap();
        assert_eq!(targets.len(), 1);
        let t = &targets[0];
        assert_eq!(t.uri.to_string(), "http://10.0.0.1:9100/metrics");
        assert_eq!(t.namespace, "ns");
        assert_eq!(t.pod_name, "p");
        assert_eq!(t.node_name.as_deref(), Some("node-a"));
        assert_eq!(t.container_name.as_deref(), Some("app"));
    }

    #[test]
    fn named_port_annotation_resolves() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/port", "metrics"),
            ],
            &[],
            vec![ContainerPort {
                container_port: 9100,
                name: Some("metrics".to_string()),
                ..Default::default()
            }],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let targets = extract_targets(&pod, &cfg).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].uri.to_string(), "http://10.0.0.1:9100/metrics");
    }

    #[test]
    fn missing_port_falls_back_to_named_metrics() {
        let pod = make_pod(
            "p",
            "ns",
            &[("prometheus.io/scrape", "true")],
            &[],
            vec![
                ContainerPort {
                    container_port: 8080,
                    name: Some("http".to_string()),
                    ..Default::default()
                },
                ContainerPort {
                    container_port: 9090,
                    name: Some("metrics".to_string()),
                    ..Default::default()
                },
            ],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let targets = extract_targets(&pod, &cfg).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].uri.to_string(), "http://10.0.0.1:9090/metrics");
    }

    #[test]
    fn missing_port_falls_back_to_first_port() {
        let pod = make_pod(
            "p",
            "ns",
            &[("prometheus.io/scrape", "true")],
            &[],
            vec![ContainerPort {
                container_port: 8080,
                ..Default::default()
            }],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let targets = extract_targets(&pod, &cfg).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].uri.to_string(), "http://10.0.0.1:8080/metrics");
    }

    #[test]
    fn custom_path_and_https_scheme() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/path", "/custom/metrics"),
                ("prometheus.io/scheme", "https"),
                ("prometheus.io/port", "9100"),
            ],
            &[],
            vec![],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let targets = extract_targets(&pod, &cfg).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].uri.to_string(),
            "https://10.0.0.1:9100/custom/metrics"
        );
    }

    #[test]
    fn invalid_scheme_returns_error() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/scheme", "gopher"),
                ("prometheus.io/port", "9100"),
            ],
            &[],
            vec![],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let err = extract_targets(&pod, &cfg).unwrap_err();
        match err {
            AnnotationError::InvalidValue { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn param_annotations_become_query_string() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/port", "9100"),
                ("prometheus.io/param_foo", "bar"),
                ("prometheus.io/param_zzz", "1"),
            ],
            &[],
            vec![],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let t = extract_targets(&pod, &cfg).unwrap().pop().unwrap();
        // Query keys are sorted for determinism.
        assert_eq!(
            t.uri.to_string(),
            "http://10.0.0.1:9100/metrics?foo=bar&zzz=1"
        );
    }

    #[test]
    fn pod_without_ip_returns_error() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/port", "9100"),
            ],
            &[],
            vec![],
            None,
            None,
        );
        let cfg = default_cfg();
        match extract_targets(&pod, &cfg).unwrap_err() {
            AnnotationError::NoPodIp => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn label_and_annotation_allowlist_extracted() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/port", "9100"),
                ("owner", "team-a"),
                ("ignored", "x"),
            ],
            &[("app", "myapp"), ("ignored-label", "y")],
            vec![],
            Some("10.0.0.1"),
            None,
        );
        let cfg = AnnotationParserConfig {
            pod_label_tags: vec!["app".to_string()],
            pod_annotation_tags: vec!["owner".to_string()],
            ..default_cfg()
        };
        let t = extract_targets(&pod, &cfg).unwrap().pop().unwrap();
        assert_eq!(t.extra_tags.get("app").map(String::as_str), Some("myapp"));
        assert_eq!(
            t.extra_tags.get("owner").map(String::as_str),
            Some("team-a")
        );
        assert!(!t.extra_tags.contains_key("ignored"));
    }

    #[test]
    fn multiple_named_metrics_ports_yield_multiple_targets() {
        let pod = make_pod(
            "p",
            "ns",
            &[("prometheus.io/scrape", "true")],
            &[],
            vec![
                ContainerPort {
                    container_port: 9100,
                    name: Some("metrics".to_string()),
                    ..Default::default()
                },
                ContainerPort {
                    container_port: 9101,
                    name: Some("http-metrics".to_string()),
                    ..Default::default()
                },
            ],
            Some("10.0.0.1"),
            None,
        );
        let cfg = default_cfg();
        let targets = extract_targets(&pod, &cfg).unwrap();
        assert_eq!(targets.len(), 2);
        let uris: Vec<_> = targets.iter().map(|t| t.uri.to_string()).collect();
        assert!(uris.contains(&"http://10.0.0.1:9100/metrics".to_string()));
        assert!(uris.contains(&"http://10.0.0.1:9101/metrics".to_string()));
    }

    #[test]
    fn honor_labels_helper_overrides_when_disabled() {
        use vector_lib::event::{Metric, MetricKind, MetricValue};
        let mut metric = Metric::new("m", MetricKind::Absolute, MetricValue::Gauge { value: 1.0 })
            .with_tags(Some({
                let mut t = vector_lib::event::metric::MetricTags::default();
                t.insert("namespace".to_string(), "from-scrape".to_string());
                t
            }));
        super::super::merge_honor_label_tag(&mut metric, "namespace", "from-discovery", false);
        assert_eq!(
            metric.tag_value("namespace").as_deref(),
            Some("from-discovery")
        );
        assert_eq!(
            metric.tag_value("exported_namespace").as_deref(),
            Some("from-scrape")
        );
    }

    #[test]
    fn honor_labels_helper_preserves_when_enabled() {
        use vector_lib::event::{Metric, MetricKind, MetricValue};
        let mut metric = Metric::new("m", MetricKind::Absolute, MetricValue::Gauge { value: 1.0 })
            .with_tags(Some({
                let mut t = vector_lib::event::metric::MetricTags::default();
                t.insert("namespace".to_string(), "from-scrape".to_string());
                t
            }));
        super::super::merge_honor_label_tag(&mut metric, "namespace", "from-discovery", true);
        assert_eq!(
            metric.tag_value("namespace").as_deref(),
            Some("from-scrape")
        );
    }

    #[test]
    fn build_field_selector_combines_parts() {
        assert_eq!(build_field_selector(None, ""), None);
        assert_eq!(
            build_field_selector(Some("n"), ""),
            Some("spec.nodeName=n".to_string())
        );
        assert_eq!(
            build_field_selector(Some("n"), "metadata.namespace=x"),
            Some("spec.nodeName=n,metadata.namespace=x".to_string())
        );
        assert_eq!(
            build_field_selector(None, "metadata.namespace=x"),
            Some("metadata.namespace=x".to_string())
        );
    }

    #[test]
    fn build_label_selector_always_has_builtin() {
        assert_eq!(build_label_selector(""), "vector.dev/exclude!=true");
        assert_eq!(build_label_selector("a=b"), "vector.dev/exclude!=true,a=b");
    }
}
