//! A metrics source that emulates the behaviour of a Prometheus Pushgateway.
//!
//! The protocol
//! [is described](https://github.com/prometheus/pushgateway/blob/master/README.md)
//! in the original Pushgateway repo, though there are some important caveats
//! to our implementation:
//!
//!   - We only support `POST` requests, not `PUT`, as the semantics of `PUT`
//!     requests in the spec aren't possible to replicate within Vector.
//!   - We don't support protobuf requests, only the Prometheus text format.
//!   - Only counters and histograms can be aggregated as there is no meaningful
//!     way to aggregate gauges or summaries.

use base64::prelude::BASE64_URL_SAFE;
use base64::Engine;
use std::{collections::HashMap, net::SocketAddr};

use bytes::Bytes;
use itertools::Itertools;
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use warp::http::HeaderMap;

use super::parser;
use crate::common::http::server_auth::HttpServerAuthConfig;
use crate::common::http::ErrorMessage;
use crate::http::KeepaliveConfig;
use crate::{
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    event::Event,
    serde::bool_or_struct,
    sources::{
        self,
        util::{http::HttpMethod, HttpSource},
    },
    tls::TlsEnableableConfig,
};

/// Configuration for the `prometheus_pushgateway` source.
#[configurable_component(source(
    "prometheus_pushgateway",
    "Receive metrics via the Prometheus Pushgateway protocol."
))]
#[derive(Clone, Debug)]
pub struct PrometheusPushgatewayConfig {
    /// The socket address to accept connections on.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:9091"))]
    address: SocketAddr,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    auth: Option<HttpServerAuthConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,

    /// Whether to aggregate values across pushes.
    ///
    /// Only applies to counters and histograms as gauges and summaries can't be
    /// meaningfully aggregated.
    #[serde(default = "crate::serde::default_false")]
    aggregate_metrics: bool,
}

impl GenerateConfig for PrometheusPushgatewayConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "127.0.0.1:9091".parse().unwrap(),
            tls: None,
            auth: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
            aggregate_metrics: false,
            keepalive: KeepaliveConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_pushgateway")]
impl SourceConfig for PrometheusPushgatewayConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = PushgatewaySource {
            aggregate_metrics: self.aggregate_metrics,
        };
        source.run(
            self.address,
            "",
            HttpMethod::Post,
            http::StatusCode::OK,
            false,
            self.tls.as_ref(),
            self.auth.as_ref(),
            cx,
            self.acknowledgements,
            self.keepalive.clone(),
        )
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Clone)]
struct PushgatewaySource {
    aggregate_metrics: bool,
}

impl PushgatewaySource {
    const fn aggregation_enabled(&self) -> bool {
        self.aggregate_metrics
    }
}

impl HttpSource for PushgatewaySource {
    fn build_events(
        &self,
        body: Bytes,
        _header_map: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let body = String::from_utf8_lossy(&body);

        let path_labels = parse_path_labels(full_path)?;

        parser::parse_text_with_overrides(&body, path_labels, self.aggregation_enabled()).map_err(
            |error| {
                ErrorMessage::new(
                    http::StatusCode::UNPROCESSABLE_ENTITY,
                    format!("Failed to parse metrics body: {}", error),
                )
            },
        )
    }
}

fn parse_path_labels(path: &str) -> Result<Vec<(String, String)>, ErrorMessage> {
    if !path.starts_with("/metrics/job") {
        return Err(ErrorMessage::new(
            http::StatusCode::BAD_REQUEST,
            "Path must begin with '/metrics/job'".to_owned(),
        ));
    }

    path.split('/')
        // Skip the first two segments as they're the empty string and
        // "metrics", which is always there as a path prefix
        .skip(2)
        .chunks(2)
        .into_iter()
        // If we get a chunk that only has 1 item, return an error
        // The path has to be made up of key-value pairs to be valid
        //
        // This includes the trailing slash case (where the single item
        // is the empty string ("") to match the real Prometheus
        // Pushgateway
        .map(|mut c| {
            c.next().zip(c.next()).ok_or_else(|| {
                ErrorMessage::new(
                    http::StatusCode::BAD_REQUEST,
                    "Request path must have an even number of segments to form grouping key"
                        .to_string(),
                )
            })
        })
        // Decode any values that have been base64 encoded per the Pushgateway spec
        //
        // See: https://github.com/prometheus/pushgateway#url
        .map(|res| res.and_then(|(k, v)| decode_label_pair(k, v)))
        .collect()
}

fn decode_label_pair(k: &str, v: &str) -> Result<(String, String), ErrorMessage> {
    // Return early if we're not dealing with a base64-encoded label
    let Some(stripped_key) = k.strip_suffix("@base64") else {
        return Ok((k.to_owned(), v.to_owned()));
    };

    // The Prometheus Pushgateway spec explicitly uses one or more `=` characters
    // (the padding character in base64) to represent an empty string in a path
    // segment:
    //
    // https://github.com/prometheus/pushgateway/blob/ec7afda4eef288bd9b9c43d063e4df54c8961272/README.md#url
    //
    // Unfortunately, the Rust base64 crate doesn't treat an encoded string that
    // only contains padding characters as valid and returns an error.
    //
    // Let's handle this case manually, before handing over to the base64 decoder.
    if v.chars().all(|c| c == '=') {
        // An empty job label isn't valid, so return an error if that's the key
        if stripped_key == "job" {
            return Err(ErrorMessage::new(
                http::StatusCode::BAD_REQUEST,
                "Job must not have an empty value".to_owned(),
            ));
        }

        return Ok((stripped_key.to_owned(), "".to_owned()));
    }

    // The Prometheus Pushgateway has a fairly permissive base64 implementation
    // that allows padding to be missing. We need to fake that by adding in
    // any missing padding before we pass the value to the base64 decoder.
    //
    // This is documented, as examples in their README don't use padding:
    //
    // https://github.com/prometheus/pushgateway/blob/ec7afda4eef288bd9b9c43d063e4df54c8961272/README.md#url
    let missing_padding = v.len() % 4;
    let padded_value = if missing_padding == 0 {
        v.to_owned()
    } else {
        let padding = "=".repeat(missing_padding);
        v.to_owned() + &padding
    };

    let decoded_bytes = BASE64_URL_SAFE.decode(padded_value).map_err(|_| {
        ErrorMessage::new(
            http::StatusCode::BAD_REQUEST,
            format!(
                "Grouping key invalid - invalid base64 value for key {}: {}",
                k, v
            ),
        )
    })?;

    let decoded = String::from_utf8(decoded_bytes).map_err(|_| {
        ErrorMessage::new(
            http::StatusCode::BAD_REQUEST,
            format!(
                "Grouping key invalid - invalid UTF-8 in decoded base64 value for key {}",
                k
            ),
        )
    })?;

    Ok((stripped_key.to_owned(), decoded))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::components::{assert_source_compliance, HTTP_PUSH_SOURCE_TAGS};
    use crate::test_util::wait_for_tcp;
    use crate::{test_util, SourceSender};
    use chrono::{TimeZone, Timelike, Utc};
    use vector_lib::event::{EventStatus, Metric, MetricKind, MetricValue};
    use vector_lib::tls::MaybeTlsSettings;

    fn events_to_metrics(events: Vec<Event>) -> Vec<Metric> {
        events.into_iter().map(Event::into_metric).collect()
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusPushgatewayConfig>();
    }

    #[test]
    fn test_parse_simple_path() {
        let path = "/metrics/job/foo/instance/bar";
        let expected: Vec<_> = vec![("job", "foo"), ("instance", "bar")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let actual = parse_path_labels(path);

        assert!(actual.is_ok());
        assert_eq!(actual.unwrap(), expected);
    }

    #[test]
    fn test_parse_path_wrong_number_of_segments() {
        let path = "/metrics/job/foo/instance";
        let result = parse_path_labels(path);

        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("number of segments"));
    }

    #[test]
    fn test_parse_path_with_base64_segment() {
        let path = "/metrics/job/foo/instance@base64/YmFyL2Jheg==";
        let expected: Vec<_> = vec![("job", "foo"), ("instance", "bar/baz")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let actual = parse_path_labels(path);

        assert!(actual.is_ok());
        assert_eq!(actual.unwrap(), expected);
    }

    #[test]
    fn test_parse_path_with_base64_segment_missing_padding() {
        let path = "/metrics/job/foo/instance@base64/YmFyL2Jheg";
        let expected: Vec<_> = vec![("job", "foo"), ("instance", "bar/baz")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let actual = parse_path_labels(path);

        assert!(actual.is_ok());
        assert_eq!(actual.unwrap(), expected);
    }

    #[test]
    fn test_parse_path_empty_job_name_invalid() {
        let path = "/metrics/job@base64/=";
        let result = parse_path_labels(path);

        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("Job must not"));
    }

    #[test]
    fn test_parse_path_empty_path_invalid() {
        let path = "/";
        let result = parse_path_labels(path);

        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("Path must begin"));
    }

    // This is to ensure that the last value for a given key is the one used when we
    // pass the grouping key into the Prometheus text parser to override label values
    // on individual metrics
    #[test]
    fn test_parse_path_duplicate_labels_preserves_order() {
        let path = "/metrics/job/foo/instance/bar/instance/baz";
        let expected: Vec<_> = vec![("job", "foo"), ("instance", "bar"), ("instance", "baz")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let actual = parse_path_labels(path);

        assert!(actual.is_ok());
        assert_eq!(actual.unwrap(), expected);
    }

    #[tokio::test]
    async fn test_whole_request_happy_path_http() {
        whole_request_happy_path(None).await;
    }

    #[tokio::test]
    async fn test_whole_request_happy_path_https() {
        whole_request_happy_path(Some(TlsEnableableConfig::test_config())).await;
    }
    async fn whole_request_happy_path(tls: Option<TlsEnableableConfig>) {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let address = test_util::next_addr();
            let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

            let source = PrometheusPushgatewayConfig {
                address,
                auth: None,
                tls: tls.clone(),
                acknowledgements: SourceAcknowledgementsConfig::default(),
                keepalive: KeepaliveConfig::default(),
                aggregate_metrics: true,
            };
            let source = source
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(source);
            wait_for_tcp(address).await;

            let proto = MaybeTlsSettings::from_config(tls.as_ref(), true)
                .unwrap()
                .http_protocol_name();
            let push_path = "metrics/job/async_worker";
            let push_url = format!(
                "{}://{}:{}/{}",
                proto,
                address.ip(),
                address.port(),
                push_path
            );
            let push_body = r#"
                # TYPE jobs_total counter
                # HELP jobs_total Total number of jobs
                jobs_total{type="a"} 1.0 1612411506789
                # TYPE jobs_current gauge
                # HELP jobs_current Current number of jobs
                jobs_current{type="a"} 5.0 1612411506789
                # TYPE jobs_distribution histogram
                # HELP jobs_distribution Distribution of jobs
                jobs_distribution_bucket{type="a",le="1"} 0.0 1612411506789
                jobs_distribution_bucket{type="a",le="2.5"} 0.0 1612411506789
                jobs_distribution_bucket{type="a",le="5"} 0.0 1612411506789
                jobs_distribution_bucket{type="a",le="10"} 1.0 1612411506789
                jobs_distribution_bucket{type="a",le="+Inf"} 1.0 1612411506789
                jobs_distribution_sum{type="a"} 8.0 1612411506789
                jobs_distribution_count{type="a"} 1.0 1612411506789
                # TYPE jobs_summary summary
                # HELP jobs_summary Summary of jobs
                jobs_summary_sum{type="a"} 8.0 1612411506789
                jobs_summary_count{type="a"} 1.0 1612411506789
                "#;

            let timestamp = Utc
                .with_ymd_and_hms(2021, 2, 4, 4, 5, 6)
                .single()
                .and_then(|t| t.with_nanosecond(789 * 1_000_000))
                .expect("invalid timestamp");

            let expected = vec![
                Metric::new(
                    "jobs_total",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some(
                    metric_tags! { "job" => "async_worker", "type" => "a" },
                ))
                .with_timestamp(Some(timestamp)),
                Metric::new(
                    "jobs_current",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 5.0 },
                )
                .with_tags(Some(
                    metric_tags! { "job" => "async_worker", "type" => "a" },
                ))
                .with_timestamp(Some(timestamp)),
                Metric::new(
                    "jobs_distribution",
                    MetricKind::Incremental,
                    MetricValue::AggregatedHistogram {
                        buckets: vector_lib::buckets![
                            1.0 => 0, 2.5 => 0, 5.0 => 0, 10.0 => 1
                        ],
                        count: 1,
                        sum: 8.0,
                    },
                )
                .with_tags(Some(
                    metric_tags! { "job" => "async_worker", "type" => "a" },
                ))
                .with_timestamp(Some(timestamp)),
                Metric::new(
                    "jobs_summary",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: vector_lib::quantiles![],
                        count: 1,
                        sum: 8.0,
                    },
                )
                .with_tags(Some(
                    metric_tags! { "job" => "async_worker", "type" => "a" },
                ))
                .with_timestamp(Some(timestamp)),
            ];

            let output = test_util::spawn_collect_ready(
                async move {
                    let client = reqwest::Client::builder()
                        .danger_accept_invalid_certs(true)
                        .build()
                        .unwrap();
                    client.post(push_url).body(push_body).send().await.unwrap();
                },
                rx,
                1,
            )
            .await;

            vector_lib::assert_event_data_eq!(expected, events_to_metrics(output));
        })
        .await;
    }
}
