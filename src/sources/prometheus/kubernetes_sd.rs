//! Kubernetes service-discovery helpers for the `prometheus_scrape` source.
//!
//! Provides Kubernetes Pod discovery via the `kube` crate and annotation-based
//! target resolution (`prometheus.io/*` annotations). Used by the
//! `prometheus_scrape` source when `targets` includes a `kubernetes` block.

use std::{collections::BTreeMap, net::IpAddr, path::PathBuf, sync::Arc, time::Duration};

use futures::{StreamExt, future};
use http::{HeaderName, HeaderValue, Uri};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Client, Config as ClientConfig,
    api::Api,
    config::{self, KubeConfigOptions},
    runtime::{WatchStreamExt, reflector, watcher},
};
use serde_with::serde_as;
use snafu::Snafu;
use tokio::task::JoinSet;
use tracing::Instrument as _;
use vector_lib::{
    configurable::configurable_component, internal_event::emit, shutdown::ShutdownSignal,
};

use crate::{
    built_info::{PKG_NAME, PKG_VERSION},
    http::QueryParameters,
    internal_events::{
        PrometheusKubernetesSdAnnotationParseError, PrometheusKubernetesSdTargetsDiscovered,
    },
    sources::util::http_client::{HttpClientTarget, Targets, build_url},
};

/// Env var consulted for the current node name when `use_self_node_only` is set.
pub(crate) const SELF_NODE_NAME_ENV_KEY: &str = "VECTOR_SELF_NODE_NAME";

const fn default_scheme() -> Scheme {
    Scheme::Http
}

fn default_path() -> String {
    "/metrics".to_string()
}

fn default_annotation_prefix() -> String {
    "prometheus.io".to_string()
}

/// Discovery role.
///
/// Only `pod` is currently supported.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Role {
    /// Discover targets from Pods.
    #[default]
    Pod,
}

/// Default scheme used when a Pod does not set `prometheus.io/scheme`.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scheme {
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

/// Kubernetes service-discovery configuration.
///
/// Controls how the source discovers scrape targets from Kubernetes Pods via
/// `prometheus.io/*` annotations.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct KubernetesScrapeConfig {
    /// Discovery role. Only `pod` is supported in this release.
    role: Role,

    /// Annotation prefix to read scrape configuration from.
    ///
    /// Defaults to `prometheus.io`, matching the de-facto Prometheus convention.
    /// The source reads `<prefix>/scrape`, `<prefix>/port`, `<prefix>/path`,
    /// `<prefix>/scheme`, and `<prefix>/param_<name>` from Pod annotations.
    #[configurable(metadata(docs::examples = "prometheus.io"))]
    pub(crate) annotation_prefix: String,

    /// Restrict discovery to Pods on the current node.
    ///
    /// When enabled, the source filters Pods by `spec.nodeName=<node>` where
    /// `<node>` is read from the `VECTOR_SELF_NODE_NAME` environment variable
    /// (or the `self_node_name` option). Use this when deploying Vector as a
    /// `DaemonSet`.
    pub(crate) use_self_node_only: bool,

    /// Override for the node name used by `use_self_node_only`.
    ///
    /// If unset and `use_self_node_only` is `true`, the `VECTOR_SELF_NODE_NAME`
    /// environment variable is read instead.
    #[configurable(metadata(docs::examples = "node-01"))]
    pub(crate) self_node_name: Option<String>,

    /// Additional field selector merged with the built-in filter.
    #[configurable(metadata(docs::examples = "metadata.namespace=monitoring"))]
    pub(crate) extra_field_selector: String,

    /// Kubernetes label selector merged with the built-in `vector.dev/exclude!=true` filter.
    ///
    /// `extra_label_selector` is accepted as a backwards-compatible alias.
    #[configurable(metadata(docs::examples = "tier=frontend"))]
    #[serde(alias = "extra_label_selector")]
    pub(crate) label_selector: String,

    /// Restrict discovery to specific namespaces.
    ///
    /// When non-empty, only Pods in the listed namespaces are watched. When
    /// empty, all namespaces are watched (requires cluster-wide RBAC).
    #[configurable(metadata(docs::examples = "monitoring"))]
    pub(crate) namespaces: Vec<String>,

    /// Path to a kubeconfig file.
    ///
    /// When unset, the source falls back to the local kubeconfig followed by
    /// in-cluster service-account credentials.
    pub(crate) kube_config_file: Option<PathBuf>,

    /// Default scheme when `<prefix>/scheme` is not set on a Pod.
    pub(crate) default_scheme: Scheme,

    /// Default scrape path when `<prefix>/path` is not set on a Pod.
    #[configurable(metadata(docs::examples = "/metrics"))]
    pub(crate) default_path: String,

    /// Allowlist of Pod labels to add as metric tags.
    ///
    /// Each entry is the label key; tags are emitted with the same key on each
    /// metric. Default is empty to avoid cardinality blow-ups.
    #[configurable(metadata(docs::examples = "app"))]
    #[configurable(metadata(docs::examples = "version"))]
    pub(crate) pod_label_tags: Vec<String>,

    /// Allowlist of Pod annotations to add as metric tags.
    #[configurable(metadata(docs::examples = "owner"))]
    pub(crate) pod_annotation_tags: Vec<String>,
}

impl Default for KubernetesScrapeConfig {
    fn default() -> Self {
        Self {
            role: Role::default(),
            annotation_prefix: default_annotation_prefix(),
            use_self_node_only: false,
            self_node_name: None,
            extra_field_selector: String::new(),
            label_selector: String::new(),
            namespaces: Vec::new(),
            kube_config_file: None,
            default_scheme: default_scheme(),
            default_path: default_path(),
            pod_label_tags: Vec::new(),
            pod_annotation_tags: Vec::new(),
        }
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

/// Build a Kubernetes client mirroring the pattern used by `kubernetes_logs`.
pub(crate) async fn build_kube_client(kube_config_file: Option<&PathBuf>) -> crate::Result<Client> {
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

pub(crate) fn build_field_selector(self_node_name: Option<&str>, extra: &str) -> Option<String> {
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

pub(crate) fn build_label_selector(extra: &str) -> String {
    const BUILT_IN: &str = "vector.dev/exclude!=true";
    if extra.is_empty() {
        BUILT_IN.to_string()
    } else {
        format!("{BUILT_IN},{extra}")
    }
}

/// Watches Pods and turns them into scrape targets.
///
/// The watcher tasks belong to this value. Dropping it, including when the source is cancelled,
/// aborts them.
pub(crate) struct PodDiscovery {
    stores: Vec<reflector::Store<Pod>>,
    parser_cfg: Arc<AnnotationParserConfig>,
    _watchers: JoinSet<()>,
}

impl PodDiscovery {
    /// Starts one watcher per namespace, or a single cluster-wide watcher when `namespaces` is
    /// empty.
    pub(crate) fn start(
        client: &Client,
        namespaces: &[String],
        field_selector: Option<String>,
        label_selector: String,
        parser_cfg: AnnotationParserConfig,
    ) -> Self {
        let watcher_cfg = watcher::Config {
            field_selector,
            label_selector: Some(label_selector),
            ..Default::default()
        };
        let apis = if namespaces.is_empty() {
            vec![Api::<Pod>::all(client.clone())]
        } else {
            namespaces
                .iter()
                .map(|namespace| Api::<Pod>::namespaced(client.clone(), namespace))
                .collect()
        };
        let mut stores = Vec::with_capacity(apis.len());
        let mut watchers = JoinSet::new();
        for api in apis {
            let writer = reflector::store::Writer::<Pod>::default();
            stores.push(writer.as_reader());
            let stream =
                watcher(api, watcher_cfg.clone()).backoff(watcher::DefaultBackoff::default());
            watchers.spawn(
                reflector::reflector(writer, stream)
                    .for_each(|result| async move {
                        if let Err(error) = result {
                            warn!(
                                message = "Watcher stream received an error. Retrying.",
                                ?error
                            );
                        }
                    })
                    .in_current_span(),
            );
        }
        Self {
            stores,
            parser_cfg: Arc::new(parser_cfg),
            _watchers: watchers,
        }
    }

    /// Waits until every watcher has finished its initial list, or until `max_wait` passes.
    /// Returns `false` if `shutdown` resolves first.
    pub(crate) async fn wait_until_ready(
        &self,
        max_wait: Duration,
        shutdown: ShutdownSignal,
    ) -> bool {
        let ready = future::join_all(self.stores.iter().map(reflector::Store::wait_until_ready));
        tokio::select! {
            result = tokio::time::timeout(max_wait, ready) => {
                if result.is_err() {
                    warn!(
                        message = "Kubernetes discovery has not finished its initial sync. Scraping the Pods found so far."
                    );
                }
                true
            }
            _ = shutdown => false,
        }
    }

    /// Returns the function that the shared scrape loop calls on every interval tick to get the
    /// current targets.
    pub(crate) fn targets(&self, query: QueryParameters) -> Targets<Target> {
        let stores = self.stores.clone();
        let parser_cfg = Arc::clone(&self.parser_cfg);
        Box::new(move || {
            let mut targets: Vec<Target> = stores
                .iter()
                .flat_map(|store| collect_targets(store, &parser_cfg))
                .collect();
            for target in &mut targets {
                target.uri = build_url(&target.uri, &query);
            }
            emit(PrometheusKubernetesSdTargetsDiscovered {
                count: targets.len(),
            });
            targets
        })
    }
}

fn collect_targets(
    store: &reflector::store::Store<Pod>,
    parser_cfg: &AnnotationParserConfig,
) -> Vec<Target> {
    let mut out = Vec::new();
    for pod in store.state() {
        match extract_targets(&pod, parser_cfg) {
            Ok(mut targets) => out.append(&mut targets),
            Err(error) => {
                emit(PrometheusKubernetesSdAnnotationParseError {
                    pod: pod.metadata.name.as_deref().unwrap_or("<unknown>"),
                    namespace: pod.metadata.namespace.as_deref().unwrap_or(""),
                    error: &error.to_string(),
                });
            }
        }
    }
    out
}

/// A single scrape target derived from a Pod annotation set or static config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) uri: Uri,
    pub(crate) instance: String,
    pub(crate) namespace: String,
    pub(crate) pod_name: String,
    pub(crate) pod_uid: String,
    pub(crate) node_name: Option<String>,
    pub(crate) container_name: Option<String>,
    /// Additional metric tags to apply (pod labels/annotations allowlist).
    pub(crate) extra_tags: BTreeMap<String, String>,
}

impl HttpClientTarget for Target {
    fn url(&self) -> &Uri {
        &self.uri
    }
}

impl Target {
    /// The tags added to every metric scraped from this target, in the order they are merged.
    pub(crate) fn discovery_tags(&self) -> Vec<(String, String)> {
        let mut tags = vec![
            ("namespace".to_owned(), self.namespace.clone()),
            ("pod".to_owned(), self.pod_name.clone()),
        ];
        if let Some(node) = &self.node_name {
            tags.push(("node".to_owned(), node.clone()));
        }
        if let Some(container) = &self.container_name {
            tags.push(("container".to_owned(), container.clone()));
        }
        tags.extend(self.extra_tags.iter().map(|(k, v)| (k.clone(), v.clone())));
        tags
    }
}

/// Annotation-parser configuration shared across ticks.
#[derive(Clone, Debug)]
pub(crate) struct AnnotationParserConfig {
    pub(crate) prefix: String,
    pub(crate) default_scheme: Scheme,
    pub(crate) default_path: String,
    pub(crate) pod_label_tags: Vec<String>,
    pub(crate) pod_annotation_tags: Vec<String>,
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
    let scrape = annotations.get(&scrape_key).map_or("false", |v| v.as_str());
    if !is_truthy(scrape) {
        return Ok(Vec::new());
    }

    let path_key = format!("{}/path", cfg.prefix);
    let path = annotations
        .get(&path_key)
        .map_or(cfg.default_path.as_str(), |s| s.as_str())
        .to_owned();

    let scheme_key = format!("{}/scheme", cfg.prefix);
    let scheme = match annotations.get(&scheme_key) {
        Some(v) => Scheme::parse(v.as_str()).ok_or_else(|| AnnotationError::InvalidValue {
            key: scheme_key.clone(),
            value: (*v).clone(),
        })?,
        None => cfg.default_scheme,
    };

    let port_key = format!("{}/port", cfg.prefix);
    let port_annotation = annotations.get(&port_key).map(|s| (*s).clone());

    // Build query string from `<prefix>/param_*` annotations.
    let param_prefix = format!("{}/param_", cfg.prefix);
    let mut query_pairs: Vec<(String, String)> = annotations
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix(param_prefix.as_str())
                .map(|name| (name.to_owned(), (*v).clone()))
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

    let ports = resolve_ports(pod, port_annotation.as_deref())
        .map_err(|message| AnnotationError::PortResolution { message })?;
    if ports.is_empty() {
        return Ok(Vec::new());
    }

    let extra_tags = build_extra_tags(pod, cfg);

    let mut targets = Vec::with_capacity(ports.len());
    for port_info in ports {
        let uri = build_target_uri(scheme, &pod_ip, port_info.port, &path, &query_pairs)?;
        let instance = format_host_port(&pod_ip, port_info.port);
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
    let Some(spec) = pod.spec.as_ref() else {
        return Ok(Vec::new());
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
                .find(|(_, p)| p.container_port == i32::from(port))
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
        .authority(format_host_port(host, port))
        .path_and_query(path_and_query)
        .build()
        .map_err(|source| AnnotationError::UriBuild { source })
}

pub(crate) fn format_host_port(host: &str, port: u16) -> String {
    if host
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_ipv6())
    {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
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
        assert_eq!(extract_targets(&pod, &cfg).unwrap(), [] as [Target; 0]);
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
        assert_eq!(extract_targets(&pod, &cfg).unwrap(), [] as [Target; 0]);
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
    fn ipv6_pod_ip_is_bracketed_in_uri_and_instance() {
        let pod = make_pod(
            "p",
            "ns",
            &[
                ("prometheus.io/scrape", "true"),
                ("prometheus.io/port", "9100"),
            ],
            &[],
            vec![],
            Some("fd00::1"),
            None,
        );

        let target = extract_targets(&pod, &default_cfg())
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(target.uri.to_string(), "http://[fd00::1]:9100/metrics");
        assert_eq!(target.instance, "[fd00::1]:9100");
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
