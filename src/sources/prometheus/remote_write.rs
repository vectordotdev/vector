use std::{collections::HashMap, net::SocketAddr};

use bytes::Bytes;
use prost::Message;
use vector_lib::{
    config::LogNamespace, configurable::configurable_component, prometheus::parser::proto,
};
use warp::http::{HeaderMap, StatusCode};

use super::parser;
use crate::{
    common::http::{ErrorMessage, server_auth::HttpServerAuthConfig},
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    event::Event,
    http::KeepaliveConfig,
    internal_events::PrometheusRemoteWriteParseError,
    serde::bool_or_struct,
    sources::{
        self,
        util::{HttpSource, decode, http::HttpMethod},
    },
    tls::TlsEnableableConfig,
};

/// Configuration for the `prometheus_remote_write` source.
#[configurable_component(source(
    "prometheus_remote_write",
    "Receive metric via the Prometheus Remote Write protocol."
))]
#[derive(Clone, Debug)]
pub struct PrometheusRemoteWriteConfig {
    /// The socket address to accept connections on.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:9090"))]
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

    /// Whether to skip/discard received samples with NaN values.
    ///
    /// When enabled, any metric sample with a NaN value will be filtered out
    /// during parsing, preventing downstream processing of invalid metrics.
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    skip_nan_values: bool,
}

impl PrometheusRemoteWriteConfig {
    #[cfg(test)]
    pub fn from_address(address: SocketAddr) -> Self {
        Self {
            address,
            tls: None,
            auth: None,
            acknowledgements: false.into(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        }
    }
}

impl GenerateConfig for PrometheusRemoteWriteConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "127.0.0.1:9090".parse().unwrap(),
            tls: None,
            auth: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SourceConfig for PrometheusRemoteWriteConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = RemoteWriteSource {
            skip_nan_values: self.skip_nan_values,
        };
        source.run(
            self.address,
            "",
            HttpMethod::Post,
            StatusCode::OK,
            true,
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
struct RemoteWriteSource {
    skip_nan_values: bool,
}

impl RemoteWriteSource {
    fn decode_body(&self, body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        let request = proto::WriteRequest::decode(body).map_err(|error| {
            emit!(PrometheusRemoteWriteParseError {
                error: error.clone()
            });
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {error}"),
            )
        })?;
        parser::parse_request(request, self.skip_nan_values).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {error}"),
            )
        })
    }
}

impl HttpSource for RemoteWriteSource {
    fn decode(&self, encoding_header: Option<&str>, body: Bytes) -> Result<Bytes, ErrorMessage> {
        // Default to snappy decoding the request body.
        decode(encoding_header.or(Some("snappy")), body)
    }

    fn build_events(
        &self,
        body: Bytes,
        _header_map: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        _full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let events = self.decode_body(body)?;
        Ok(events)
    }
}

#[cfg(test)]
mod test {
    use chrono::{SubsecRound as _, Utc};
    use vector_lib::{
        event::{EventStatus, Metric, MetricKind, MetricValue},
        metric_tags,
    };

    use super::*;
    use crate::{
        SourceSender,
        config::{SinkConfig, SinkContext},
        sinks::prometheus::remote_write::RemoteWriteConfig,
        test_util::{self, wait_for_tcp},
        tls::MaybeTlsSettings,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusRemoteWriteConfig>();
    }

    #[tokio::test]
    async fn receives_metrics_over_http() {
        receives_metrics(None).await;
    }

    #[tokio::test]
    async fn receives_metrics_over_https() {
        receives_metrics(Some(TlsEnableableConfig::test_config())).await;
    }

    async fn receives_metrics(tls: Option<TlsEnableableConfig>) {
        let address = test_util::next_addr();
        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

        let proto = MaybeTlsSettings::from_config(tls.as_ref(), true)
            .unwrap()
            .http_protocol_name();
        let source = PrometheusRemoteWriteConfig {
            address,
            auth: None,
            tls: tls.clone(),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        };
        let source = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(source);
        wait_for_tcp(address).await;

        let sink = RemoteWriteConfig {
            endpoint: format!("{}://localhost:{}/", proto, address.port()),
            tls: tls.map(|tls| tls.options),
            ..Default::default()
        };
        let (sink, _) = sink
            .build(SinkContext::default())
            .await
            .expect("Error building config.");

        let events = make_events();
        let events_copy = events.clone();
        let mut output = test_util::spawn_collect_ready(
            async move {
                sink.run_events(events_copy).await.unwrap();
            },
            rx,
            1,
        )
        .await;

        // The MetricBuffer used by the sink may reorder the metrics, so
        // put them back into order before comparing.
        output.sort_unstable_by_key(|event| event.as_metric().name().to_owned());

        vector_lib::assert_event_data_eq!(events, output);
    }

    fn make_events() -> Vec<Event> {
        let timestamp = || Utc::now().trunc_subsecs(3);
        vec![
            Metric::new(
                "counter_1",
                MetricKind::Absolute,
                MetricValue::Counter { value: 42.0 },
            )
            .with_timestamp(Some(timestamp()))
            .into(),
            Metric::new(
                "gauge_2",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 41.0 },
            )
            .with_timestamp(Some(timestamp()))
            .into(),
            Metric::new(
                "histogram_3",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vector_lib::buckets![ 2.3 => 11, 4.2 => 85 ],
                    count: 96,
                    sum: 156.2,
                },
            )
            .with_timestamp(Some(timestamp()))
            .into(),
            Metric::new(
                "summary_4",
                MetricKind::Absolute,
                MetricValue::AggregatedSummary {
                    quantiles: vector_lib::quantiles![ 0.1 => 1.2, 0.5 => 3.6, 0.9 => 5.2 ],
                    count: 23,
                    sum: 8.6,
                },
            )
            .with_timestamp(Some(timestamp()))
            .into(),
        ]
    }

    async fn send_request_and_assert(port: u16, request_body: Vec<u8>) {
        // Send the request via HTTP POST
        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://localhost:{}/", port))
            .header("Content-Type", "application/x-protobuf")
            .header("Content-Encoding", "snappy")
            .body(request_body)
            .send()
            .await
            .unwrap();

        // Should succeed (not return 400) despite conflicting metadata
        assert!(
            response.status().is_success(),
            "Expected success but got: {}",
            response.status()
        );
    }

    /// According to the [spec](https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md?plain=1#L115)
    /// > Label names MUST be unique within a LabelSet.
    /// Prometheus itself will reject the metric with an error. Largely to remain backward compatible with older versions of Vector,
    /// we accept the metric, but take the last label in the list.
    #[tokio::test]
    async fn receives_metrics_duplicate_labels() {
        let address = test_util::next_addr();
        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

        let source = PrometheusRemoteWriteConfig {
            address,
            auth: None,
            tls: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        };
        let source = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(source);
        wait_for_tcp(address).await;

        let sink = RemoteWriteConfig {
            endpoint: format!("http://localhost:{}/", address.port()),
            ..Default::default()
        };
        let (sink, _) = sink
            .build(SinkContext::default())
            .await
            .expect("Error building config.");

        let timestamp = Utc::now().trunc_subsecs(3);

        let events = vec![
            Metric::new(
                "gauge_2",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 41.0 },
            )
            .with_timestamp(Some(timestamp))
            .with_tags(Some(metric_tags! {
                "code" => "200".to_string(),
                "code" => "success".to_string(),
            }))
            .into(),
        ];

        let expected = vec![
            Metric::new(
                "gauge_2",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 41.0 },
            )
            .with_timestamp(Some(timestamp))
            .with_tags(Some(metric_tags! {
                "code" => "success".to_string(),
            }))
            .into(),
        ];

        let output = test_util::spawn_collect_ready(
            async move {
                sink.run_events(events).await.unwrap();
            },
            rx,
            1,
        )
        .await;

        vector_lib::assert_event_data_eq!(expected, output);
    }

    #[tokio::test]
    async fn test_skip_nan_values_enabled() {
        let address = test_util::next_addr();
        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

        let source = PrometheusRemoteWriteConfig {
            address,
            auth: None,
            tls: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: true,
        };
        let source = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(source);
        wait_for_tcp(address).await;

        // Create a request with NaN values
        let request_body = {
            use prost::Message;
            use vector_lib::prometheus::parser::proto;

            let request = proto::WriteRequest {
                metadata: vec![],
                timeseries: vec![
                    proto::TimeSeries {
                        labels: vec![proto::Label {
                            name: "__name__".into(),
                            value: "test_metric_valid".into(),
                        }],
                        samples: vec![proto::Sample {
                            value: 42.0,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        }],
                    },
                    proto::TimeSeries {
                        labels: vec![proto::Label {
                            name: "__name__".into(),
                            value: "test_metric_nan".into(),
                        }],
                        samples: vec![proto::Sample {
                            value: f64::NAN,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        }],
                    },
                ],
            };

            let mut buf = Vec::new();
            request.encode(&mut buf).unwrap();

            // Compress with snappy as expected by the remote_write endpoint
            snap::raw::Encoder::new().compress_vec(&buf).unwrap()
        };

        send_request_and_assert(address.port(), request_body).await;

        // Verify we only received the valid metric (NaN metric should be filtered)
        let output = test_util::collect_ready(rx).await;
        assert_eq!(output.len(), 1);

        let metric = output[0].as_metric();
        assert_eq!(metric.name(), "test_metric_valid");
        assert_eq!(metric.value(), &MetricValue::Gauge { value: 42.0 });
    }

    #[tokio::test]
    async fn test_skip_nan_values_disabled() {
        let address = test_util::next_addr();
        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

        let source = PrometheusRemoteWriteConfig {
            address,
            auth: None,
            tls: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        };
        let source = source
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();
        tokio::spawn(source);
        wait_for_tcp(address).await;

        // Create a request with NaN values
        let request_body = {
            use prost::Message;
            use vector_lib::prometheus::parser::proto;

            let request = proto::WriteRequest {
                metadata: vec![],
                timeseries: vec![
                    proto::TimeSeries {
                        labels: vec![proto::Label {
                            name: "__name__".into(),
                            value: "test_metric_valid".into(),
                        }],
                        samples: vec![proto::Sample {
                            value: 42.0,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        }],
                    },
                    proto::TimeSeries {
                        labels: vec![proto::Label {
                            name: "__name__".into(),
                            value: "test_metric_nan".into(),
                        }],
                        samples: vec![proto::Sample {
                            value: f64::NAN,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        }],
                    },
                ],
            };

            let mut buf = Vec::new();
            request.encode(&mut buf).unwrap();

            // Compress with snappy as expected by the remote_write endpoint
            snap::raw::Encoder::new().compress_vec(&buf).unwrap()
        };

        send_request_and_assert(address.port(), request_body).await;

        // Verify we received both metrics (including NaN metric)
        let mut output = test_util::collect_ready(rx).await;
        assert_eq!(output.len(), 2);

        // Sort by name for predictable testing
        output.sort_by(|a, b| a.as_metric().name().cmp(b.as_metric().name()));

        // Check the NaN metric
        let nan_metric = output[0].as_metric();
        assert_eq!(nan_metric.name(), "test_metric_nan");
        match nan_metric.value() {
            MetricValue::Gauge { value } => {
                assert!(value.is_nan());
            }
            _ => panic!("Expected gauge metric"),
        }

        // Check the valid metric
        let valid_metric = output[1].as_metric();
        assert_eq!(valid_metric.name(), "test_metric_valid");
        assert_eq!(valid_metric.value(), &MetricValue::Gauge { value: 42.0 });
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use std::net::{SocketAddr, ToSocketAddrs as _};

    use tokio::time::Duration;

    use super::*;
    use crate::test_util::components::{HTTP_PUSH_SOURCE_TAGS, run_and_assert_source_compliance};

    fn source_receive_address() -> SocketAddr {
        let address = std::env::var("REMOTE_WRITE_SOURCE_RECEIVE_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1:9102".into());
        // TODO: This logic should maybe be moved up into the source, and possibly into other
        // sources, wrapped in a new socket address type that does the lookup during config parsing.
        address
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap_or_else(|| panic!("Socket address {address:?} did not resolve"))
    }

    #[tokio::test]
    async fn receive_something() {
        // TODO: This test depends on the single instance of Prometheus that we spin up for
        // integration tests both scraping an endpoint and then also remote writing that stuff to
        // this remote write source.  This makes sense from a "test the actual behavior" standpoint
        // but it feels a little fragile.
        //
        // It could be nice to split up the Prometheus integration tests in the future, or
        // maybe there's a way to do a one-shot remote write from Prometheus? Not sure.
        let config = PrometheusRemoteWriteConfig {
            address: source_receive_address(),
            auth: None,
            tls: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(5),
            &HTTP_PUSH_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());
    }
}
