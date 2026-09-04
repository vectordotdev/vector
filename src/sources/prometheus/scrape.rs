use std::{collections::HashMap, time::Duration};

use bytes::Bytes;
use futures_util::FutureExt;
use http::{Uri, response::Parts};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use vector_lib::{config::LogNamespace, configurable::configurable_component, event::Event};

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

/// Errors returned when a `scrape_delay` value cannot be parsed.
#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub(crate) enum ScrapeDelayParseError {
    #[snafu(display("A delay in seconds must be a valid integer"))]
    SecondsParse,
    #[snafu(display("The delay is too large to schedule"))]
    DelayTooLarge,
    // last case evaluated must explain all valid formats accepted
    #[snafu(display("Must be \"none\", \"auto\", or a delay in seconds such as \"30s\""))]
    UnableToParse,
}

/// When scrapes happen, relative to the configured scrape interval.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
// No `#[serde(untagged)]`: `try_from`/`into` already make this (de)serialize as a plain string, so
// the enum representation is never used, and `Configurable` rejects untagged enums that have more
// than one unit variant.
#[serde(try_from = "String", into = "String")]
#[configurable(metadata(docs::examples = "none"))]
#[configurable(metadata(docs::examples = "auto"))]
#[configurable(metadata(docs::examples = "30s"))]
pub(crate) enum ScrapeDelay {
    /// Scrape as soon as the source starts, then once every `scrape_interval_secs` exactly.
    #[default]
    None,

    /// Scrape once per interval, choosing a position inside each interval independently.
    ///
    /// Under normal polling, the source starts one scrape round in each interval, but it does not
    /// remain at one fixed phase. Intervals missed while the schedule is not polled are skipped
    /// rather than replayed. This reduces persistent alignment with other periodic work. Two
    /// consecutive scrape starts can be anywhere from nearly zero to nearly two intervals apart,
    /// which can increase short-lived overlap and load compared with a fixed cadence. The scheduler
    /// does not enforce a minimum gap between starts or place an upper bound on in-flight scrapes.
    ///
    /// The positions come from a hash of the host name, the component ID and the scrape number
    /// rather than from a random number, so the sequence is reproducible relative to source start.
    /// Different seeds normally produce different sequences, but individual scrapes can still
    /// coincide, and instances with the same host name and component ID use the same sequence.
    Auto,

    /// Scrape after exactly this many seconds, written as `30s`.
    ///
    /// This is the manual counterpart to `auto`: give each source a different value to stagger them
    /// by hand.
    Fixed(Duration),
}

impl TryFrom<String> for ScrapeDelay {
    type Error = ScrapeDelayParseError;

    fn try_from(input: String) -> std::result::Result<Self, Self::Error> {
        match input.as_str() {
            "none" => Ok(Self::None),
            "auto" => Ok(Self::Auto),
            s => match s.strip_suffix('s') {
                Some(secs) => {
                    let delay = secs
                        .parse::<u64>()
                        .map(Duration::from_secs)
                        .map_err(|_| Self::Error::SecondsParse)?;

                    std::time::Instant::now()
                        .checked_add(delay)
                        .map(|_| Self::Fixed(delay))
                        .ok_or(Self::Error::DelayTooLarge)
                }
                None => Err(Self::Error::UnableToParse),
            },
        }
    }
}

impl From<ScrapeDelay> for String {
    fn from(delay: ScrapeDelay) -> String {
        match delay {
            ScrapeDelay::None => "none".to_owned(),
            ScrapeDelay::Auto => "auto".to_owned(),
            ScrapeDelay::Fixed(delay) => format!("{}s", delay.as_secs()),
        }
    }
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
    #[configurable(metadata(docs::examples = "http://localhost:9090/metrics"))]
    #[serde(alias = "hosts")]
    endpoints: Vec<String>,

    /// The interval between scrapes. Requests are run concurrently so if a scrape takes longer
    /// than the interval a new scrape will be started. This can take extra resources, set the timeout
    /// to a value lower than the scrape interval to prevent this from happening.
    #[serde(default = "default_interval")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    interval: Duration,

    /// When scrapes happen, relative to the configured scrape interval.
    ///
    /// `none` scrapes as soon as the source starts and then every `scrape_interval_secs` exactly,
    /// which means the source raises load at one unvarying period, and sources that share an
    /// interval all scrape at the same instant.
    ///
    /// A delay in seconds, such as `30s`, holds the first scrape back by exactly that much and then
    /// keeps the same fixed cadence. Give each source a different value to stagger them by hand.
    ///
    /// `auto` chooses a position inside each interval independently. Under normal polling, the
    /// source starts one scrape round in each interval, but it does not remain at one fixed phase;
    /// intervals missed while the schedule is not polled are skipped rather than replayed. This
    /// reduces persistent alignment with other periodic work and with sources sharing the same
    /// interval. Two consecutive scrape starts can be anywhere from nearly zero to nearly two
    /// intervals apart, which can increase short-lived overlap and load compared with a fixed
    /// cadence. The scheduler does not enforce a minimum gap between starts or place an upper bound
    /// on in-flight scrapes. The positions come from a hash of the host name, the component ID and
    /// the scrape number rather than from a random number, so the sequence is reproducible relative
    /// to source start. Hash-derived positions do not guarantee distinct slots: individual scrapes
    /// can still coincide, and instances with the same host name and component ID use the same
    /// sequence.
    #[serde(default)]
    #[configurable(metadata(docs::human_name = "Scrape Delay"))]
    scrape_delay: ScrapeDelay,

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
    /// This matches Prometheus’ `honor_labels` configuration.
    #[serde(default = "crate::serde::default_false")]
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

    tls: Option<TlsConfig>,

    auth: Option<Auth>,
}

impl PrometheusScrapeConfig {
    /// The delay applied before the first scrape.
    ///
    /// Only the first scrape is moved; every scrape after it is driven by the configured interval,
    /// so this shifts the phase of the source without ever changing the spacing between samples.
    ///
    /// `auto` sits with `none` here on purpose. It has no separate initial offset: its first scrape
    /// is simply the first of the jittered ones, placed inside the first interval by the schedule
    /// itself.
    const fn first_scrape_delay(&self) -> Duration {
        match self.scrape_delay {
            ScrapeDelay::None | ScrapeDelay::Auto => Duration::ZERO,
            ScrapeDelay::Fixed(delay) => delay,
        }
    }
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
            interval: default_interval(),
            scrape_delay: ScrapeDelay::None,
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

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_scrape")]
impl SourceConfig for PrometheusScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        let urls = self
            .endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| build_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;
        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        let builder = PrometheusScrapeBuilder {
            honor_labels: self.honor_labels,
            instance_tag: self.instance_tag.clone(),
            endpoint_tag: self.endpoint_tag.clone(),
        };

        warn_if_interval_too_low(self.timeout, self.interval);

        // Picks where inside each interval this source scrapes. The component ID gives sources in
        // this Vector instance different deterministic sequences, and the host name does the same
        // for instances with different host names. These inputs reduce persistent alignment but do
        // not guarantee distinct positions. If the host name cannot be read, only the component ID
        // differentiates the sequences.
        let delay_seed = format!(
            "{}\0{}",
            crate::get_hostname().unwrap_or_default(),
            cx.key.id()
        );

        let first_scrape_delay = self.first_scrape_delay();

        let inputs = GenericHttpClientInputs {
            urls,
            interval: self.interval,
            initial_delay: first_scrape_delay,
            jitter_seed: match self.scrape_delay {
                ScrapeDelay::Auto => Some(delay_seed),
                ScrapeDelay::None | ScrapeDelay::Fixed(_) => None,
            },
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
                match (honor_label, metric.tag_value(tag)) {
                    (false, Some(old_instance)) => {
                        metric.replace_tag(format!("exported_{tag}"), old_instance);
                        metric.replace_tag(tag.clone(), instance.clone());
                    }
                    (true, Some(_)) => {}
                    (_, None) => {
                        metric.replace_tag(tag.clone(), instance.clone());
                    }
                }
            }
            if let Some(EndpointInfo {
                tag,
                endpoint,
                honor_label,
            }) = &self.endpoint_info
            {
                match (honor_label, metric.tag_value(tag)) {
                    (false, Some(old_endpoint)) => {
                        metric.replace_tag(format!("exported_{tag}"), old_endpoint);
                        metric.replace_tag(tag.clone(), endpoint.clone());
                    }
                    (true, Some(_)) => {}
                    (_, None) => {
                        metric.replace_tag(tag.clone(), endpoint.clone());
                    }
                }
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
    use tokio::time::{Duration, sleep, timeout};
    use warp::Filter;

    use super::*;
    use crate::{
        Error, SourceSender, config,
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

    #[test]
    fn scrape_delay_defaults_to_none() {
        let config: PrometheusScrapeConfig = toml::from_str(
            r#"
                endpoints = ["http://localhost:9090/metrics"]
            "#,
        )
        .unwrap();

        assert_eq!(config.scrape_delay, ScrapeDelay::None);
    }

    #[test]
    fn scrape_delay_parses_every_form() {
        let parse = |delay: &str| {
            let config: PrometheusScrapeConfig = toml::from_str(&format!(
                r#"
                    endpoints = ["http://localhost:9090/metrics"]
                    scrape_delay = "{delay}"
                "#
            ))
            .unwrap();

            config.scrape_delay
        };

        assert_eq!(parse("none"), ScrapeDelay::None);
        assert_eq!(parse("auto"), ScrapeDelay::Auto);
        assert_eq!(parse("30s"), ScrapeDelay::Fixed(Duration::from_secs(30)));
        assert_eq!(parse("0s"), ScrapeDelay::Fixed(Duration::ZERO));
    }

    #[test]
    fn scrape_delay_rejects_invalid_values() {
        for delay in ["30", "auto+30s", "thirty seconds", "", "s"] {
            let parsed = ScrapeDelay::try_from(delay.to_string());

            assert!(
                parsed.is_err(),
                "expected `{delay}` to be rejected, got {parsed:?}"
            );
        }

        // Well-formed, but too far out to turn into a schedule.
        assert_eq!(
            ScrapeDelay::try_from(format!("{}s", u64::MAX)),
            Err(ScrapeDelayParseError::DelayTooLarge)
        );
    }

    #[test]
    fn scrape_delay_round_trips_through_a_string() {
        for delay in ["none", "auto", "30s"] {
            let parsed = ScrapeDelay::try_from(delay.to_string()).unwrap();

            assert_eq!(String::from(parsed), delay);
        }
    }

    #[test]
    fn only_a_fixed_delay_holds_back_the_first_scrape() {
        let first_scrape_delay = |delay| {
            let config: PrometheusScrapeConfig = toml::from_str(&format!(
                r#"
                    endpoints = ["http://localhost:9090/metrics"]
                    scrape_interval_secs = 60
                    scrape_delay = "{delay}"
                "#
            ))
            .unwrap();

            config.first_scrape_delay()
        };

        // A fixed delay is taken literally; the interval has no say in it.
        assert_eq!(first_scrape_delay("30s"), Duration::from_secs(30));

        // `auto` has no initial offset of its own to add. Its first scrape is the first of the
        // jittered ones, which the schedule already places somewhere inside the first interval;
        // adding a delay on top would only push every scrape out of its interval.
        assert_eq!(first_scrape_delay("auto"), Duration::ZERO);

        assert_eq!(first_scrape_delay("none"), Duration::ZERO);
    }

    #[tokio::test]
    async fn scrape_delay_defers_the_first_request() {
        let (_guard, in_addr) = next_addr();
        let (request_tx, mut request_rx) = tokio::sync::mpsc::unbounded_channel();
        let dummy_endpoint = warp::path!("metrics").map(move || {
            request_tx.send(()).unwrap();
            "test_metric 1\n"
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let source_config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            interval: Duration::from_secs(10),
            scrape_delay: ScrapeDelay::Fixed(Duration::from_secs(2)),
            timeout: default_timeout(),
            instance_tag: None,
            endpoint_tag: None,
            honor_labels: false,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };
        let (out, _events) = SourceSender::new_test();
        let source = source_config
            .build(SourceContext::new_test(out, None))
            .await
            .unwrap();
        let source_task = tokio::spawn(source);

        assert!(
            timeout(Duration::from_millis(500), request_rx.recv())
                .await
                .is_err()
        );
        assert_eq!(
            timeout(Duration::from_secs(3), request_rx.recv())
                .await
                .unwrap(),
            Some(())
        );

        source_task.abort();
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
            interval: Duration::from_secs(1),
            scrape_delay: ScrapeDelay::None,
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
            interval: Duration::from_secs(1),
            scrape_delay: ScrapeDelay::None,
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
            interval: Duration::from_secs(1),
            scrape_delay: ScrapeDelay::None,
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
            interval: Duration::from_secs(1),
            scrape_delay: ScrapeDelay::None,
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
            interval: Duration::from_secs(1),
            scrape_delay: ScrapeDelay::None,
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
                instance_tag: None,
                endpoint_tag: None,
                honor_labels: false,
                query: HashMap::new(),
                interval: Duration::from_secs(1),
                scrape_delay: ScrapeDelay::None,
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
            interval: Duration::from_secs(1),
            scrape_delay: ScrapeDelay::None,
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
