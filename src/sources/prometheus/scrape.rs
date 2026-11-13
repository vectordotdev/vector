use std::{collections::HashMap, time::Duration};

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, StatusCode, Uri, response::Parts, uri::Scheme};
use serde_with::serde_as;
use snafu::ResultExt;
use vector_lib::{
    config::LogNamespace, configurable::configurable_component, event::Event, internal_event::emit,
    shutdown::ShutdownSignal,
};

#[cfg(feature = "kubernetes")]
use futures::{StreamExt, stream};
#[cfg(feature = "kubernetes")]
use tokio_stream::wrappers::IntervalStream;

#[cfg(feature = "kubernetes")]
use super::kubernetes_sd::{self, KubernetesScrapeConfig};
use super::parser;
#[cfg(feature = "kubernetes")]
use crate::sources::util::http_client::{RequestOptions, run};
use crate::{
    Result,
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    http::{Auth, HttpClient, QueryParameters},
    internal_events::PrometheusParseError,
    sources::{
        self,
        util::{
            http::HttpMethod,
            http_client::{
                GenericHttpClientInputs, HttpClientBuilder, HttpClientContext, build_headers,
                build_url, call, default_interval, default_timeout, warn_if_interval_too_low,
            },
        },
    },
    tls::{TlsConfig, TlsSettings},
};

// pulled up, and split over multiple lines, because the long lines trip up rustfmt such that it
// gave up trying to format, but reported no error
static PARSE_ERROR_NO_PATH: &str = "No path is set on the endpoint and we got a parse error,\
                                    did you mean to use /metrics? This behavior changed in version 0.11.";
static NOT_FOUND_NO_PATH: &str = "No path is set on the endpoint and we got a 404,\
                                  did you mean to use /metrics?\
                                  This behavior changed in version 0.11.";

/// Auto-discover scrape targets. Each entry describes a target group.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of targets to discover."))]
pub(crate) enum TargetConfig {
    /// A static list of scrape URLs.
    Static {
        /// URLs to scrape.
        #[configurable(metadata(docs::examples = "http://localhost:9090/metrics"))]
        urls: Vec<String>,
    },
    /// Kubernetes Pod auto-discovery via `prometheus.io/*` annotations.
    #[cfg(feature = "kubernetes")]
    Kubernetes(KubernetesScrapeConfig),
}

/// Configuration for the `prometheus_scrape` source.
#[serde_as]
#[configurable_component(source(
    "prometheus_scrape",
    "Collect metrics from Prometheus exporters."
))]
#[derive(Clone, Debug)]
pub struct PrometheusScrapeConfig {
    /// Endpoints to scrape metrics from.
    ///
    /// Deprecated: use `targets` with a `static` block instead.
    #[configurable(metadata(docs::examples = "http://localhost:9090/metrics"))]
    #[serde(alias = "hosts", default)]
    endpoints: Vec<String>,

    /// Auto-discover scrape targets.
    ///
    /// Each entry in the list configures a target group. Supported types are
    /// `static` (fixed URLs) and `kubernetes` (Pod annotation discovery).
    #[serde(default)]
    targets: Vec<TargetConfig>,

    /// The interval between scrapes. Requests are run concurrently so if a scrape takes longer
    /// than the interval a new scrape will be started. This can take extra resources, set the timeout
    /// to a value lower than the scrape interval to prevent this from happening.
    #[serde(default = "default_interval")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    interval: Duration,

    /// The timeout for each scrape request.
    #[serde(default = "default_timeout")]
    #[serde_as(as = "serde_with:: DurationSecondsWithFrac<f64>")]
    #[serde(rename = "scrape_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Timeout"))]
    timeout: Duration,

    /// The tag name added to each event representing the scraped instance's `host:port`.
    ///
    /// The tag value is the host and port of the scraped instance.
    instance_tag: Option<String>,

    /// The tag name added to each event representing the scraped instance's endpoint.
    ///
    /// The tag value is the endpoint of the scraped instance.
    endpoint_tag: Option<String>,

    /// Controls how tag conflicts are handled if the scraped source has tags to be added.
    ///
    /// If `true`, the new tag is not added if the scraped metric has the tag already. If `false`, the conflicting tag
    /// is renamed by prepending `exported_` to the original name.
    ///
    /// This matches Prometheus' `honor_labels` configuration.
    #[serde(default = "crate::serde::default_false")]
    honor_labels: bool,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
    /// scraping the `/federate` endpoint.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "query_example()"))]
    query: QueryParameters,

    /// Headers to apply to the scrape requests.
    ///
    /// One or more values for the same header can be provided.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and its value(s)."
    ))]
    #[configurable(metadata(docs::examples = "headers_example()"))]
    headers: HashMap<String, Vec<String>>,

    tls: Option<TlsConfig>,

    auth: Option<Auth>,
}

fn query_example() -> serde_json::Value {
    serde_json::json! ({
        "match[]": [
            "{job=\"somejob\"}",
            "{__name__=~\"job:.*\"}"
        ]
    })
}

fn headers_example() -> serde_json::Value {
    serde_json::json!({
        "X-My-Header": ["value1", "value2"]
    })
}

impl GenerateConfig for PrometheusScrapeConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            endpoints: vec!["http://localhost:9090/metrics".to_string()],
            targets: vec![],
            interval: default_interval(),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            headers: HashMap::new(),
            tls: None,
            auth: None,
        })
        .unwrap()
    }
}

#[derive(Debug, snafu::Snafu)]
enum ConfigError {
    #[snafu(display(
        "exactly one of `endpoints` or `targets` must be specified (\"endpoints\" is deprecated, prefer \"targets\")"
    ))]
    EndpointsAndTargetsConflict,
    #[snafu(display("at least one endpoint or target must be specified"))]
    NoEndpointsOrTargets,
    #[snafu(display("only one Kubernetes target group is currently supported"))]
    MultipleKubernetesTargetGroups,
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_scrape")]
impl SourceConfig for PrometheusScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        warn_if_interval_too_low(self.timeout, self.interval);

        // Collect static URLs
        let mut static_urls: Vec<Uri> = Vec::new();
        for s in &self.endpoints {
            let uri = s.parse::<Uri>().context(sources::UriParseSnafu)?;
            static_urls.push(uri);
        }
        for target in &self.targets {
            match target {
                TargetConfig::Static { urls } => {
                    for s_url in urls {
                        let uri = s_url.parse::<Uri>().context(sources::UriParseSnafu)?;
                        static_urls.push(uri);
                    }
                }
                #[cfg(feature = "kubernetes")]
                TargetConfig::Kubernetes(_) => {}
            }
        }

        let static_urls: Vec<Uri> = static_urls
            .iter()
            .map(|uri| build_url(uri, &self.query))
            .collect();
        let tls = TlsSettings::from_options(self.tls.as_ref())?;
        let headers = build_headers(&self.headers)?;
        let client = HttpClient::new(tls.into(), &cx.proxy)?;
        let builder = PrometheusScrapeBuilder {
            honor_labels: self.honor_labels,
            instance_tag: self.instance_tag.clone(),
            endpoint_tag: self.endpoint_tag.clone(),
        };

        #[cfg(feature = "kubernetes")]
        if let Some(k8s_cfg) = self.targets.iter().find_map(|target| match target {
            TargetConfig::Kubernetes(k8s_cfg) => Some(k8s_cfg),
            TargetConfig::Static { .. } => None,
        }) {
            return self
                .build_kubernetes_source(k8s_cfg, static_urls, headers, client, builder, cx)
                .await;
        }

        let inputs = self.http_client_inputs(static_urls, headers, client, cx.shutdown);
        Ok(crate::sources::source_with_error(call(
            inputs,
            builder,
            cx.out,
            HttpMethod::Get,
        )))
    }

    fn validate_structure(&self) -> std::result::Result<(), Vec<String>> {
        let has_endpoints = !self.endpoints.is_empty();
        let has_configured_targets = self.has_any_targets();
        let mut errors = Vec::new();

        if has_endpoints && has_configured_targets {
            errors.push(ConfigError::EndpointsAndTargetsConflict.to_string());
        } else if !has_endpoints && !has_configured_targets {
            errors.push(ConfigError::NoEndpointsOrTargets.to_string());
        }

        #[cfg(feature = "kubernetes")]
        if self
            .targets
            .iter()
            .filter(|target| matches!(target, TargetConfig::Kubernetes(_)))
            .count()
            > 1
        {
            errors.push(ConfigError::MultipleKubernetesTargetGroups.to_string());
        }

        #[cfg(feature = "kubernetes")]
        for target in &self.targets {
            if let TargetConfig::Kubernetes(k8s_cfg) = target {
                errors.extend(
                    k8s_cfg
                        .validate_tags(self.instance_tag.as_deref(), self.endpoint_tag.as_deref()),
                );
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl PrometheusScrapeConfig {
    fn has_any_targets(&self) -> bool {
        self.targets.iter().any(|target| match target {
            TargetConfig::Static { urls } => !urls.is_empty(),
            #[cfg(feature = "kubernetes")]
            TargetConfig::Kubernetes(_) => true,
        })
    }

    fn http_client_inputs(
        &self,
        urls: Vec<Uri>,
        headers: HeaderMap<HeaderValue>,
        client: HttpClient,
        shutdown: ShutdownSignal,
    ) -> GenericHttpClientInputs {
        GenericHttpClientInputs {
            urls,
            interval: self.interval,
            timeout: self.timeout,
            headers,
            content_type: "text/plain".to_string(),
            auth: self.auth.clone(),
            client,
            shutdown,
        }
    }

    /// Builds the source when `targets` contains a Kubernetes group.
    ///
    /// Static URLs and discovered Pods are scraped by two loops that share the source's output.
    /// The source ends when either loop ends, which also stops the Pod watchers.
    #[cfg(feature = "kubernetes")]
    async fn build_kubernetes_source(
        &self,
        k8s_cfg: &KubernetesScrapeConfig,
        static_urls: Vec<Uri>,
        headers: HeaderMap<HeaderValue>,
        client: HttpClient,
        builder: PrometheusScrapeBuilder,
        cx: SourceContext,
    ) -> Result<sources::Source> {
        let kube_client =
            kubernetes_sd::build_kube_client(k8s_cfg.kube_config_file.as_ref()).await?;

        let self_node_name = if k8s_cfg.use_self_node_only {
            let resolved = match k8s_cfg.self_node_name.clone() {
                Some(n) => n,
                None => std::env::var(kubernetes_sd::SELF_NODE_NAME_ENV_KEY).map_err(|_| {
                    format!(
                        "self_node_name config value or {} env var must be set when use_self_node_only is true",
                        kubernetes_sd::SELF_NODE_NAME_ENV_KEY
                    )
                })?,
            };
            Some(resolved)
        } else {
            None
        };

        let discovery = kubernetes_sd::PodDiscovery::start(
            &kube_client,
            &k8s_cfg.namespaces,
            kubernetes_sd::build_field_selector(
                self_node_name.as_deref(),
                &k8s_cfg.extra_field_selector,
            ),
            kubernetes_sd::build_label_selector(&k8s_cfg.label_selector),
            kubernetes_sd::AnnotationParserConfig {
                prefix: k8s_cfg.annotation_prefix.clone(),
                default_scheme: k8s_cfg.default_scheme,
                default_path: k8s_cfg.default_path.clone(),
                pod_label_tags: k8s_cfg.pod_label_tags.clone(),
                pod_annotation_tags: k8s_cfg.pod_annotation_tags.clone(),
            },
        );

        let static_scrape = (!static_urls.is_empty()).then(|| {
            call(
                self.http_client_inputs(
                    static_urls,
                    headers.clone(),
                    client.clone(),
                    cx.shutdown.clone(),
                ),
                builder.clone(),
                cx.out.clone(),
                HttpMethod::Get,
            )
        });

        let options = RequestOptions {
            client,
            method: HttpMethod::Get,
            timeout: self.timeout,
            headers,
            content_type: "text/plain".to_string(),
            auth: self.auth.clone(),
        };
        let interval = self.interval;
        let query = self.query.clone();
        let shutdown = cx.shutdown;
        let out = cx.out;
        let discovered_scrape = async move {
            // Start once the initial Pod list is in, so the first scrape isn't empty.
            if !discovery.wait_until_ready(interval, shutdown.clone()).await {
                return Ok(());
            }
            // Read the current Pods on every tick. `discovery` owns the watchers, so it lives
            // as long as this loop.
            let requests = IntervalStream::new(tokio::time::interval(interval))
                .take_until(shutdown)
                .flat_map(|_| stream::iter(discovery.targets(&query)))
                .map(|target| {
                    let context = builder.context_for(&target);
                    (target.uri, context)
                });
            run(requests, options, out).await
        };

        Ok(sources::source_with_error(async move {
            match static_scrape {
                Some(static_scrape) => futures::future::try_join(static_scrape, discovered_scrape)
                    .await
                    .map(|((), ())| ()),
                None => discovered_scrape.await,
            }
        }))
    }
}

// InstanceInfo stores the scraped instance info and the tag to insert into the log event with. It
// is used to join these two pieces of info to avoid storing the instance if instance_tag is not
// configured
#[derive(Clone)]
struct InstanceInfo {
    tag: String,
    instance: String,
    honor_label: bool,
}

// EndpointInfo stores the scraped endpoint info and the tag to insert into the log event with. It
// is used to join these two pieces of info to avoid storing the endpoint if endpoint_tag is not
// configured
#[derive(Clone)]
struct EndpointInfo {
    tag: String,
    endpoint: String,
    honor_label: bool,
}

/// Captures the configuration options required to build request-specific context.
#[derive(Clone)]
struct PrometheusScrapeBuilder {
    honor_labels: bool,
    instance_tag: Option<String>,
    endpoint_tag: Option<String>,
}

impl PrometheusScrapeBuilder {
    /// Builds the context with the instance info and endpoint info for the current request.
    fn context(&self, url: &Uri, instance: String) -> PrometheusScrapeContext {
        let instance_info = self.instance_tag.as_ref().map(|tag| InstanceInfo {
            tag: tag.clone(),
            instance,
            honor_label: self.honor_labels,
        });
        let endpoint_info = self.endpoint_tag.as_ref().map(|tag| EndpointInfo {
            tag: tag.clone(),
            endpoint: url.to_string(),
            honor_label: self.honor_labels,
        });
        PrometheusScrapeContext {
            instance_info,
            endpoint_info,
            honor_labels: self.honor_labels,
            discovery_tags: Vec::new(),
        }
    }
}

impl HttpClientBuilder for PrometheusScrapeBuilder {
    type Context = PrometheusScrapeContext;

    fn build(&self, url: &Uri) -> Self::Context {
        let instance = format!(
            "{}:{}",
            url.host().unwrap_or_default(),
            url.port_u16().unwrap_or_else(|| match url.scheme() {
                Some(scheme) if scheme == &Scheme::HTTP => 80,
                Some(scheme) if scheme == &Scheme::HTTPS => 443,
                _ => 0,
            })
        );
        self.context(url, instance)
    }
}

#[cfg(feature = "kubernetes")]
impl PrometheusScrapeBuilder {
    /// Builds the context for a discovered Pod target, which adds the Pod's tags.
    fn context_for(&self, target: &kubernetes_sd::Target) -> PrometheusScrapeContext {
        let mut context = self.context(&target.uri, target.instance.clone());
        context.discovery_tags = target.discovery_tags();
        context
    }
}

/// Request-specific context required for decoding into events.
struct PrometheusScrapeContext {
    instance_info: Option<InstanceInfo>,
    endpoint_info: Option<EndpointInfo>,
    honor_labels: bool,
    /// Tags from Kubernetes discovery, such as `namespace` and `pod`. Empty for static targets.
    discovery_tags: Vec<(String, String)>,
}

impl HttpClientContext for PrometheusScrapeContext {
    fn enrich_events(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            let metric = event.as_mut_metric();
            if let Some(InstanceInfo {
                tag,
                instance,
                honor_label,
            }) = &self.instance_info
            {
                super::merge_honor_label_tag(metric, tag, instance, *honor_label);
            }
            if let Some(EndpointInfo {
                tag,
                endpoint,
                honor_label,
            }) = &self.endpoint_info
            {
                super::merge_honor_label_tag(metric, tag, endpoint, *honor_label);
            }
            for (tag, value) in &self.discovery_tags {
                super::merge_honor_label_tag(metric, tag, value, self.honor_labels);
            }
        }
    }

    /// Parses the Prometheus HTTP response into metric events
    fn on_response(&mut self, url: &Uri, _header: &Parts, body: &Bytes) -> Option<Vec<Event>> {
        let body = String::from_utf8_lossy(body);

        match parser::parse_text(&body) {
            Ok(events) => Some(events),
            Err(error) => {
                if url.path() == "/" {
                    // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
                    warn!(
                        message = PARSE_ERROR_NO_PATH,
                        endpoint = %url,
                    );
                }
                emit(PrometheusParseError {
                    error,
                    url: url.clone(),
                    body,
                });
                None
            }
        }
    }

    fn on_http_response_error(&self, url: &Uri, header: &Parts) {
        if header.status == StatusCode::NOT_FOUND && url.path() == "/" {
            // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
            warn!(
                message = NOT_FOUND_NO_PATH,
                endpoint = %url,
            );
        }
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::config;

    fn config_with(endpoints: Vec<String>, targets: Vec<TargetConfig>) -> PrometheusScrapeConfig {
        PrometheusScrapeConfig {
            endpoints,
            targets,
            interval: default_interval(),
            timeout: default_timeout(),
            instance_tag: None,
            endpoint_tag: None,
            honor_labels: false,
            query: HashMap::new(),
            headers: HashMap::new(),
            tls: None,
            auth: None,
        }
    }

    fn compilation_errors(config: PrometheusScrapeConfig) -> Vec<String> {
        let mut builder = config::Config::builder();
        builder.add_source("prometheus", config);
        builder.build().expect_err("config should be rejected")
    }

    #[test]
    fn requires_exactly_one_endpoint_or_target_during_config_compilation() {
        let errors = compilation_errors(config_with(Vec::new(), Vec::new()));
        assert!(errors.iter().any(|error| {
            error == "Source prometheus at least one endpoint or target must be specified"
        }));

        let errors = compilation_errors(config_with(
            vec!["http://localhost:9090/metrics".to_string()],
            vec![TargetConfig::Static {
                urls: vec!["http://localhost:9091/metrics".to_string()],
            }],
        ));
        assert!(errors.iter().any(|error| {
            error == "Source prometheus exactly one of `endpoints` or `targets` must be specified (\"endpoints\" is deprecated, prefer \"targets\")"
        }));
    }

    #[cfg(feature = "kubernetes")]
    #[test]
    fn rejects_multiple_kubernetes_groups_during_config_compilation() {
        let errors = compilation_errors(config_with(
            Vec::new(),
            vec![
                TargetConfig::Kubernetes(KubernetesScrapeConfig::default()),
                TargetConfig::Kubernetes(KubernetesScrapeConfig::default()),
            ],
        ));
        assert!(errors.iter().any(|error| {
            error == "Source prometheus only one Kubernetes target group is currently supported"
        }));
    }

    #[cfg(feature = "kubernetes")]
    #[test]
    fn rejects_conflicting_kubernetes_tags_during_config_compilation() {
        let mut k8s_cfg = KubernetesScrapeConfig::default();
        k8s_cfg.pod_label_tags = vec!["namespace".to_owned()];
        let errors = compilation_errors(config_with(
            Vec::new(),
            vec![TargetConfig::Kubernetes(k8s_cfg)],
        ));
        assert!(
            errors
                .iter()
                .any(|error| error.starts_with("Source prometheus ")
                    && error.contains("`pod_label_tags` entry `namespace`")),
            "{errors:?}"
        );
    }

    /// A scraped metric that already has `exported_namespace` keeps the static path's behavior:
    /// with `honor_labels` off, the metric's own `namespace` replaces it.
    #[cfg(feature = "kubernetes")]
    #[test]
    fn discovery_tags_follow_honor_labels_like_static_tags() {
        use vector_lib::event::{Metric, MetricKind, MetricValue};

        let builder = PrometheusScrapeBuilder {
            honor_labels: false,
            instance_tag: Some("instance".to_owned()),
            endpoint_tag: None,
        };
        let target = kubernetes_sd::Target {
            uri: "http://10.0.0.1:9100/metrics".parse().unwrap(),
            instance: "10.0.0.1:9100".to_owned(),
            namespace: "monitoring".to_owned(),
            pod_name: "exporter-0".to_owned(),
            pod_uid: "uid".to_owned(),
            node_name: None,
            container_name: None,
            extra_tags: std::collections::BTreeMap::new(),
        };
        let mut metric = Metric::new(
            "up",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        );
        metric.replace_tag("namespace".to_owned(), "app".to_owned());
        metric.replace_tag("exported_namespace".to_owned(), "old".to_owned());
        let mut events = vec![Event::Metric(metric)];

        builder.context_for(&target).enrich_events(&mut events);

        let metric = events[0].as_metric();
        assert_eq!(metric.tag_value("namespace").as_deref(), Some("monitoring"));
        assert_eq!(
            metric.tag_value("exported_namespace").as_deref(),
            Some("app")
        );
        assert_eq!(metric.tag_value("pod").as_deref(), Some("exporter-0"));
        assert_eq!(
            metric.tag_value("instance").as_deref(),
            Some("10.0.0.1:9100")
        );
    }
}

#[cfg(all(test, feature = "sinks-prometheus"))]
mod test {
    use std::convert::Infallible;

    use http::Response;
    use http_body_util::{BodyExt as _, Empty, Full};
    use hyper_util::{client::legacy::Client, rt::TokioExecutor};
    use similar_asserts::assert_eq;
    use tokio::time::{Duration, sleep};
    use vector_lib::event::Metric;
    use warp::Filter;

    use super::*;
    use crate::{
        config::{self, AcknowledgementsConfig},
        http::{ParameterValue, QueryParameterValue},
        sinks::prometheus::exporter::PrometheusExporterConfig,
        test_util::{
            addr::next_addr,
            components::{
                COMPONENT_ERROR_TAGS, HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance,
                run_and_assert_source_error,
            },
            http::{SlowBody, spawn_blackhole_http_server, spawn_slow_body_http_server},
            start_topology, trace_init, wait_for_tcp,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusScrapeConfig>();
    }

    #[tokio::test]
    async fn test_prometheus_sets_headers() {
        let (_guard, in_addr) = next_addr();

        let dummy_endpoint = warp::path!("metrics")
            .and(warp::header::exact("Accept", "application/openmetrics-text"))
            .and(warp::header::exact("X-My-Header", "custom-value"))
            .map(|| {
                r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                    "#
            });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{in_addr}/metrics")],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            headers: HashMap::from([
                (
                    "Accept".to_string(),
                    vec!["application/openmetrics-text".to_string()],
                ),
                ("X-My-Header".to_string(), vec!["custom-value".to_string()]),
            ]),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);
    }

    #[tokio::test]
    async fn test_prometheus_stalled_body_times_out_without_blocking_other_endpoints() {
        let stalled = spawn_slow_body_http_server(SlowBody::Stall).await;

        let (_guard, in_addr) = next_addr();
        let healthy_endpoint = warp::path!("metrics").map(|| {
            r#"
                promhttp_metric_handler_requests_total{code="200"} 100 1612411516789
                "#
        });
        tokio::spawn(warp::serve(healthy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![stalled.to_string(), format!("http://{in_addr}/metrics")],
            targets: Vec::new(),
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            headers: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events =
            run_and_assert_source_error(config, Duration::from_secs(3), &COMPONENT_ERROR_TAGS)
                .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);
    }

    #[tokio::test]
    async fn test_prometheus_honor_labels() {
        let (_guard, in_addr) = next_addr();

        let dummy_endpoint = warp::path!("metrics").map(|| {
                r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                    "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{in_addr}/metrics")],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            headers: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);

        let metrics: Vec<_> = events.into_iter().map(Event::into_metric).collect();

        for metric in metrics {
            assert_eq!(
                metric.tag_value("instance"),
                Some(String::from("localhost:9999"))
            );
            assert_eq!(
                metric.tag_value("endpoint"),
                Some(String::from("http://example.com"))
            );
            assert_eq!(metric.tag_value("exported_instance"), None,);
            assert_eq!(metric.tag_value("exported_endpoint"), None,);
        }
    }

    #[tokio::test]
    async fn test_prometheus_do_not_honor_labels() {
        let (_guard, in_addr) = next_addr();

        let dummy_endpoint = warp::path!("metrics").map(|| {
                r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{in_addr}/metrics")],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            headers: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);

        let metrics: Vec<_> = events.into_iter().map(Event::into_metric).collect();

        for metric in metrics {
            assert_eq!(
                metric.tag_value("instance"),
                Some(format!("{}:{}", in_addr.ip(), in_addr.port()))
            );
            assert_eq!(
                metric.tag_value("endpoint"),
                Some(format!(
                    "http://{}:{}/metrics",
                    in_addr.ip(),
                    in_addr.port()
                ))
            );
            assert_eq!(
                metric.tag_value("exported_instance"),
                Some(String::from("localhost:9999"))
            );
            assert_eq!(
                metric.tag_value("exported_endpoint"),
                Some(String::from("http://example.com"))
            );
        }
    }

    /// According to the [spec](https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md?plain=1#L115)
    /// > Label names MUST be unique within a LabelSet.
    /// Prometheus itself will reject the metric with an error. Largely to remain backward compatible with older versions of Vector,
    /// we accept the metric, but take the last label in the list.
    #[tokio::test]
    async fn test_prometheus_duplicate_tags() {
        let (_guard, in_addr) = next_addr();

        let dummy_endpoint = warp::path!("metrics").map(|| {
            r#"
                    metric_label{code="200",code="success"} 100 1612411516789
            "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{in_addr}/metrics")],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            headers: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);

        let metrics: Vec<Metric> = events.into_iter().map(Event::into_metric).collect();
        let metric = &metrics[0];

        assert_eq!(metric.name(), "metric_label");

        let code_tag = metric
            .tags()
            .unwrap()
            .iter_all()
            .filter(|(name, _value)| *name == "code")
            .map(|(_name, value)| value)
            .collect::<Vec<_>>();

        assert_eq!(1, code_tag.len());
        assert_eq!("success", code_tag[0].unwrap());
    }

    #[tokio::test]
    async fn test_prometheus_request_query() {
        let (_guard, in_addr) = next_addr();

        let dummy_endpoint = warp::path!("metrics").and(warp::query::raw()).map(|query| {
            format!(
                r#"
                    promhttp_metric_handler_requests_total{{query="{query}"}} 100 1612411516789
                "#
            )
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{in_addr}/metrics?key1=val1")],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::from([
                (
                    "key1".to_string(),
                    QueryParameterValue::MultiParams(vec![ParameterValue::String(
                        "val2".to_string(),
                    )]),
                ),
                (
                    "key2".to_string(),
                    QueryParameterValue::MultiParams(vec![
                        ParameterValue::String("val1".to_string()),
                        ParameterValue::String("val2".to_string()),
                    ]),
                ),
            ]),
            headers: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);

        let metrics: Vec<_> = events.into_iter().map(Event::into_metric).collect();

        let expected = HashMap::from([
            (
                "key1".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
            (
                "key2".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
        ]);

        for metric in metrics {
            let query = metric.tag_value("query").expect("query must be tagged");
            let mut got: HashMap<String, Vec<String>> = HashMap::new();
            for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
                got.entry(k.to_string()).or_default().push(v.to_string());
            }
            for v in got.values_mut() {
                v.sort();
            }
            assert_eq!(got, expected);
        }
    }

    // Intentionally not using assert_source_compliance here because this is a round-trip test which
    // means source and sink will both emit `EventsSent` , triggering multi-emission check.
    #[allow(
        clippy::too_many_lines,
        reason = "Splitting this fixture is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
    )]
    #[tokio::test]
    async fn test_prometheus_routing() {
        trace_init();
        let (_out_guard, out_addr) = next_addr();

        let in_uri = spawn_blackhole_http_server(|_| async {
            Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(
                r#"
                    # HELP promhttp_metric_handler_requests_total Total number of scrapes by HTTP status code.
                    # TYPE promhttp_metric_handler_requests_total counter
                    promhttp_metric_handler_requests_total{code="200"} 100 1612411516789
                    promhttp_metric_handler_requests_total{code="404"} 7 1612411516789
                    prometheus_remote_storage_samples_in_total 57011636 1612411516789
                    # A histogram, which has a pretty complex representation in the text format:
                    # HELP http_request_duration_seconds A histogram of the request duration.
                    # TYPE http_request_duration_seconds histogram
                    http_request_duration_seconds_bucket{le="0.05"} 24054 1612411516789
                    http_request_duration_seconds_bucket{le="0.1"} 33444 1612411516789
                    http_request_duration_seconds_bucket{le="0.2"} 100392 1612411516789
                    http_request_duration_seconds_bucket{le="0.5"} 129389 1612411516789
                    http_request_duration_seconds_bucket{le="1"} 133988 1612411516789
                    http_request_duration_seconds_bucket{le="+Inf"} 144320 1612411516789
                    http_request_duration_seconds_sum 53423 1612411516789
                    http_request_duration_seconds_count 144320 1612411516789
                    # Finally a summary, which has a complex representation, too:
                    # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
                    # TYPE rpc_duration_seconds summary
                    rpc_duration_seconds{code="200",quantile="0.01"} 3102 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.05"} 3272 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.5"} 4773 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.9"} 9001 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.99"} 76656 1612411516789
                    rpc_duration_seconds_sum{code="200"} 1.7560473e+07 1612411516789
                    rpc_duration_seconds_count{code="200"} 2693 1612411516789
                    "#
                .as_bytes(),
            ))))
        })
        .await;
        let in_addr = in_uri.authority().unwrap();

        let mut config = config::Config::builder();
        config.add_source(
            "in",
            PrometheusScrapeConfig {
                endpoints: vec![format!("http://{in_addr}")],
                targets: vec![],
                instance_tag: None,
                endpoint_tag: None,
                honor_labels: false,
                query: HashMap::new(),
                headers: HashMap::new(),
                interval: Duration::from_secs(1),
                timeout: default_timeout(),
                tls: None,
                auth: None,
            },
        );
        config.add_sink(
            "out",
            &["in"],
            PrometheusExporterConfig {
                address: out_addr,
                auth: None,
                tls: None,
                default_namespace: Some("vector".into()),
                buckets: vec![1.0, 2.0, 4.0],
                quantiles: vec![],
                distributions_as_summaries: false,
                flush_period_secs: Duration::from_secs(3),
                suppress_timestamp: false,
                acknowledgements: AcknowledgementsConfig::default(),
            },
        );

        let (topology, _) = start_topology(config.build().unwrap(), false).await;
        sleep(Duration::from_secs(1)).await;

        let response = Client::builder(TokioExecutor::new())
            .build_http::<Empty<Bytes>>()
            .get(format!("http://{out_addr}/metrics").parse().unwrap())
            .await
            .unwrap();

        assert!(response.status().is_success());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        assert_eq!(
            lines,
            vec![
                "# HELP vector_http_request_duration_seconds http_request_duration_seconds",
                "# TYPE vector_http_request_duration_seconds histogram",
                "vector_http_request_duration_seconds_bucket{le=\"0.05\"} 24054 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"0.1\"} 33444 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"0.2\"} 100392 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"0.5\"} 129389 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"1\"} 133988 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"+Inf\"} 144320 1612411516789",
                "vector_http_request_duration_seconds_sum 53423 1612411516789",
                "vector_http_request_duration_seconds_count 144320 1612411516789",
                "# HELP vector_prometheus_remote_storage_samples_in_total prometheus_remote_storage_samples_in_total",
                "# TYPE vector_prometheus_remote_storage_samples_in_total gauge",
                "vector_prometheus_remote_storage_samples_in_total 57011636 1612411516789",
                "# HELP vector_promhttp_metric_handler_requests_total promhttp_metric_handler_requests_total",
                "# TYPE vector_promhttp_metric_handler_requests_total counter",
                "vector_promhttp_metric_handler_requests_total{code=\"200\"} 100 1612411516789",
                "vector_promhttp_metric_handler_requests_total{code=\"404\"} 7 1612411516789",
                "# HELP vector_rpc_duration_seconds rpc_duration_seconds",
                "# TYPE vector_rpc_duration_seconds summary",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.01\"} 3102 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.05\"} 3272 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.5\"} 4773 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.9\"} 9001 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.99\"} 76656 1612411516789",
                "vector_rpc_duration_seconds_sum{code=\"200\"} 17560473 1612411516789",
                "vector_rpc_duration_seconds_count{code=\"200\"} 2693 1612411516789",
            ],
        );

        topology.stop().await;
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use tokio::time::Duration;

    use super::*;
    use crate::{
        event::{MetricKind, MetricValue},
        test_util::components::{HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance},
    };

    #[tokio::test]
    async fn scrapes_metrics() {
        let config = PrometheusScrapeConfig {
            endpoints: vec!["http://prometheus:9090/metrics".into()],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            headers: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert_ne!(events, [] as [vector_lib::event::Event; 0]);

        let metrics: Vec<_> = events.into_iter().map(Event::into_metric).collect();

        let find_metric = |name: &str| {
            metrics
                .iter()
                .find(|metric| metric.name() == name)
                .unwrap_or_else(|| panic!("Missing metric {name:?}"))
        };

        // Sample some well-known metrics
        let build = find_metric("prometheus_build_info");
        assert!(matches!(build.kind(), MetricKind::Absolute));
        assert!(matches!(build.value(), &MetricValue::Gauge { .. }));
        assert!(build.tags().unwrap().contains_key("branch"));
        assert!(build.tags().unwrap().contains_key("version"));
        assert_eq!(
            build.tag_value("instance"),
            Some("prometheus:9090".to_string())
        );
        assert_eq!(
            build.tag_value("endpoint"),
            Some("http://prometheus:9090/metrics".to_string())
        );

        let queries = find_metric("prometheus_engine_queries");
        assert!(matches!(queries.kind(), MetricKind::Absolute));
        assert!(matches!(queries.value(), &MetricValue::Gauge { .. }));
        assert_eq!(
            queries.tag_value("instance"),
            Some("prometheus:9090".to_string())
        );
        assert_eq!(
            queries.tag_value("endpoint"),
            Some("http://prometheus:9090/metrics".to_string())
        );

        let go_info = find_metric("go_info");
        assert!(matches!(go_info.kind(), MetricKind::Absolute));
        assert!(matches!(go_info.value(), &MetricValue::Gauge { .. }));
        assert!(go_info.tags().unwrap().contains_key("version"));
        assert_eq!(
            go_info.tag_value("instance"),
            Some("prometheus:9090".to_string())
        );
        assert_eq!(
            go_info.tag_value("endpoint"),
            Some("http://prometheus:9090/metrics".to_string())
        );
    }
}
