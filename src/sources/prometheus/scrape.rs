use std::{collections::HashMap, time::Duration};

use bytes::Bytes;
use futures_util::FutureExt;
use http::{Uri, response::Parts};
use serde_with::serde_as;
use snafu::ResultExt;
use vector_lib::{config::LogNamespace, configurable::configurable_component, event::Event};

#[cfg(feature = "kubernetes")]
use super::kubernetes_sd::{self, KubernetesScrapeConfig};
use super::parser;
use crate::{
    Result,
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    http::{Auth, QueryParameters},
    internal_events::PrometheusParseError,
    sources::{
        self,
        util::{
            http::HttpMethod,
            http_client::{
                GenericHttpClientInputs, HttpClientBuilder, HttpClientContext, build_url, call,
                default_interval, default_timeout, warn_if_interval_too_low,
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
pub enum TargetConfig {
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
    #[configurable(metadata(docs::advanced))]
    instance_tag: Option<String>,

    /// The tag name added to each event representing the scraped instance's endpoint.
    ///
    /// The tag value is the endpoint of the scraped instance.
    #[configurable(metadata(docs::advanced))]
    endpoint_tag: Option<String>,

    /// Controls how tag conflicts are handled if the scraped source has tags to be added.
    ///
    /// If `true`, the new tag is not added if the scraped metric has the tag already. If `false`, the conflicting tag
    /// is renamed by prepending `exported_` to the original name.
    ///
    /// This matches Prometheus' `honor_labels` configuration.
    #[serde(default = "crate::serde::default_false")]
    #[configurable(metadata(docs::advanced))]
    honor_labels: bool,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
    /// scraping the `/federate` endpoint.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    #[configurable(metadata(docs::examples = "query_example()"))]
    query: QueryParameters,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
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
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_scrape")]
impl SourceConfig for PrometheusScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        // Validate: exactly one of endpoints or targets must be non-empty.
        let has_endpoints = !self.endpoints.is_empty();
        let has_configured_targets = self.has_any_targets();

        if has_endpoints && has_configured_targets {
            return Err(Box::new(ConfigError::EndpointsAndTargetsConflict));
        }
        if !has_endpoints && !has_configured_targets {
            return Err(Box::new(ConfigError::NoEndpointsOrTargets));
        }

        warn_if_interval_too_low(self.timeout, self.interval);

        // Collect static URLs
        let mut static_urls: Vec<Uri> = Vec::new();
        for s in &self.endpoints {
            let uri = s.parse::<Uri>().context(sources::UriParseSnafu)?;
            static_urls.push(build_url(&uri, &self.query));
        }
        for target in &self.targets {
            match target {
                TargetConfig::Static { urls } => {
                    for s_url in urls {
                        let uri = s_url.parse::<Uri>().context(sources::UriParseSnafu)?;
                        static_urls.push(build_url(&uri, &self.query));
                    }
                }
                #[cfg(feature = "kubernetes")]
                TargetConfig::Kubernetes(_) => {}
            }
        }

        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        #[cfg(feature = "kubernetes")]
        {
            let kubernetes_cfgs: Vec<&KubernetesScrapeConfig> = self
                .targets
                .iter()
                .filter_map(|t| {
                    if let TargetConfig::Kubernetes(k) = t {
                        Some(k)
                    } else {
                        None
                    }
                })
                .collect();

            if !kubernetes_cfgs.is_empty() {
                let k8s_cfg = kubernetes_cfgs[0];

                let client =
                    kubernetes_sd::build_kube_client(k8s_cfg.kube_config_file.as_ref()).await?;

                let self_node_name = if k8s_cfg.use_self_node_only {
                    let resolved = match k8s_cfg.self_node_name.clone() {
                        Some(n) => n,
                        None => std::env::var(kubernetes_sd::SELF_NODE_NAME_ENV_KEY).map_err(
                            |_| {
                                let msg = format!(
                                    "self_node_name config value or {} env var must be set when use_self_node_only is true",
                                    kubernetes_sd::SELF_NODE_NAME_ENV_KEY
                                );
                                msg
                            },
                        )?,
                    };
                    Some(resolved)
                } else {
                    None
                };

                let field_selector = kubernetes_sd::build_field_selector(
                    self_node_name.as_deref(),
                    &k8s_cfg.extra_field_selector,
                );
                let label_selector =
                    kubernetes_sd::build_label_selector(&k8s_cfg.extra_label_selector);

                let parser_cfg = kubernetes_sd::AnnotationParserConfig {
                    prefix: k8s_cfg.annotation_prefix.clone(),
                    default_scheme: k8s_cfg.default_scheme,
                    default_path: k8s_cfg.default_path.clone(),
                    pod_label_tags: k8s_cfg.pod_label_tags.clone(),
                    pod_annotation_tags: k8s_cfg.pod_annotation_tags.clone(),
                };

                let scrape_cfg = kubernetes_sd::ScrapeConfig {
                    interval: self.interval,
                    timeout: self.timeout,
                    instance_tag: self.instance_tag.clone(),
                    endpoint_tag: self.endpoint_tag.clone(),
                    honor_labels: self.honor_labels,
                    auth: self.auth.clone(),
                };

                let delay_deletion = Duration::from_millis(k8s_cfg.delay_deletion_ms);

                // Convert static URLs to Target structs.
                let static_targets: Vec<kubernetes_sd::Target> = static_urls
                    .into_iter()
                    .map(|uri| {
                        let instance = format!(
                            "{}:{}",
                            uri.host().unwrap_or_default(),
                            uri.port_u16().unwrap_or_else(|| match uri.scheme() {
                                Some(scheme) if scheme == &http::uri::Scheme::HTTP => 80,
                                Some(scheme) if scheme == &http::uri::Scheme::HTTPS => 443,
                                _ => 0,
                            })
                        );
                        kubernetes_sd::Target {
                            uri: uri.clone(),
                            instance,
                            is_static: true,
                            namespace: String::new(),
                            pod_name: String::new(),
                            pod_uid: String::new(),
                            node_name: None,
                            container_name: None,
                            extra_tags: std::collections::BTreeMap::new(),
                        }
                    })
                    .collect();

                return Ok(Box::pin(kubernetes_sd::run(
                    client,
                    tls,
                    cx.proxy.clone(),
                    k8s_cfg.namespaces.clone(),
                    field_selector,
                    label_selector,
                    delay_deletion,
                    parser_cfg,
                    scrape_cfg,
                    static_targets,
                    cx.out,
                    cx.shutdown,
                )));
            }
        }

        // Pure static path: use existing `call()`.
        let builder = PrometheusScrapeBuilder {
            honor_labels: self.honor_labels,
            instance_tag: self.instance_tag.clone(),
            endpoint_tag: self.endpoint_tag.clone(),
        };

        let inputs = GenericHttpClientInputs {
            urls: static_urls,
            interval: self.interval,
            timeout: self.timeout,
            headers: HashMap::new(),
            content_type: "text/plain".to_string(),
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(call(inputs, builder, cx.out, HttpMethod::Get).boxed())
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
        for target in &self.targets {
            match target {
                TargetConfig::Static { urls } if !urls.is_empty() => return true,
                #[cfg(feature = "kubernetes")]
                TargetConfig::Kubernetes(_) => return true,
                #[allow(unreachable_patterns)]
                TargetConfig::Static { .. } => {}
            }
        }
        false
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

impl HttpClientBuilder for PrometheusScrapeBuilder {
    type Context = PrometheusScrapeContext;

    /// Expands the context with the instance info and endpoint info for the current request.
    fn build(&self, url: &Uri) -> Self::Context {
        let instance_info = self.instance_tag.as_ref().map(|tag| {
            let instance = format!(
                "{}:{}",
                url.host().unwrap_or_default(),
                url.port_u16().unwrap_or_else(|| match url.scheme() {
                    Some(scheme) if scheme == &http::uri::Scheme::HTTP => 80,
                    Some(scheme) if scheme == &http::uri::Scheme::HTTPS => 443,
                    _ => 0,
                })
            );
            InstanceInfo {
                tag: tag.to_string(),
                instance,
                honor_label: self.honor_labels,
            }
        });
        let endpoint_info = self.endpoint_tag.as_ref().map(|tag| EndpointInfo {
            tag: tag.to_string(),
            endpoint: url.to_string(),
            honor_label: self.honor_labels,
        });
        PrometheusScrapeContext {
            instance_info,
            endpoint_info,
        }
    }
}

/// Request-specific context required for decoding into events.
struct PrometheusScrapeContext {
    instance_info: Option<InstanceInfo>,
    endpoint_info: Option<EndpointInfo>,
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
                emit!(PrometheusParseError {
                    error,
                    url: url.clone(),
                    body,
                });
                None
            }
        }
    }

    fn on_http_response_error(&self, url: &Uri, header: &Parts) {
        if header.status == hyper::StatusCode::NOT_FOUND && url.path() == "/" {
            // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
            warn!(
                message = NOT_FOUND_NO_PATH,
                endpoint = %url,
            );
        }
    }
}

#[cfg(all(test, feature = "sinks-prometheus"))]
mod test {
    use http_body::Body as _;
    use hyper::{
        Body, Client, Response, Server,
        service::{make_service_fn, service_fn},
    };
    use similar_asserts::assert_eq;
    use tokio::time::{Duration, sleep};
    use warp::Filter;

    use super::*;
    use crate::{
        Error, config,
        http::{ParameterValue, QueryParameterValue},
        sinks::prometheus::exporter::PrometheusExporterConfig,
        test_util::{
            addr::next_addr,
            components::{HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance},
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

        let dummy_endpoint = warp::path!("metrics").and(warp::header::exact("Accept", "text/plain")).map(|| {
            r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                    "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());
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
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

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
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

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
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            targets: vec![],
            interval: Duration::from_secs(1),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<vector_lib::event::Metric> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();
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
            endpoints: vec![format!("http://{}/metrics?key1=val1", in_addr)],
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
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

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
    #[tokio::test]
    async fn test_prometheus_routing() {
        trace_init();
        let (_in_guard, in_addr) = next_addr();
        let (_out_guard, out_addr) = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(Response::new(Body::from(
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
                    "#,
                )))
            }))
        });

        tokio::spawn(async move {
            if let Err(error) = Server::bind(&in_addr).serve(make_svc).await {
                error!(message = "Server error.", %error);
            }
        });
        wait_for_tcp(in_addr).await;

        let mut config = config::Config::builder();
        config.add_source(
            "in",
            PrometheusScrapeConfig {
                endpoints: vec![format!("http://{}", in_addr)],
                targets: vec![],
                instance_tag: None,
                endpoint_tag: None,
                honor_labels: false,
                query: HashMap::new(),
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
                acknowledgements: Default::default(),
            },
        );

        let (topology, _) = start_topology(config.build().unwrap(), false).await;
        sleep(Duration::from_secs(1)).await;

        let response = Client::new()
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
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

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
