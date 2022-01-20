use std::{collections::HashMap, net::SocketAddr};

use bytes::Bytes;
use prometheus_parser::proto;
use prost::Message;
use serde::{Deserialize, Serialize};
use warp::http::{HeaderMap, StatusCode};

use super::parser;
use crate::{
    config::{
        self, AcknowledgementsConfig, GenerateConfig, Output, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::Event,
    internal_events::PrometheusRemoteWriteParseError,
    serde::bool_or_struct,
    sources::{
        self,
        util::{decode, ErrorMessage, HttpSource, HttpSourceAuthConfig},
    },
    tls::TlsConfig,
};

const SOURCE_NAME: &str = "prometheus_remote_write";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PrometheusRemoteWriteConfig {
    address: SocketAddr,

    tls: Option<TlsConfig>,

    auth: Option<HttpSourceAuthConfig>,

    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

inventory::submit! {
    SourceDescription::new::<PrometheusRemoteWriteConfig>(SOURCE_NAME)
}

impl GenerateConfig for PrometheusRemoteWriteConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "127.0.0.1:9090".parse().unwrap(),
            tls: None,
            auth: None,
            acknowledgements: AcknowledgementsConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SourceConfig for PrometheusRemoteWriteConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = RemoteWriteSource;
        source.run(
            self.address,
            "",
            true,
            &self.tls,
            &self.auth,
            cx,
            self.acknowledgements,
        )
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(config::DataType::Metric)]
    }

    fn source_type(&self) -> &'static str {
        SOURCE_NAME
    }
}

#[derive(Clone)]
struct RemoteWriteSource;

impl RemoteWriteSource {
    fn decode_body(&self, body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        let request = proto::WriteRequest::decode(body).map_err(|error| {
            emit!(&PrometheusRemoteWriteParseError {
                error: error.clone()
            });
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {}", error),
            )
        })?;
        parser::parse_request(request).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {}", error),
            )
        })
    }
}

impl HttpSource for RemoteWriteSource {
    fn build_events(
        &self,
        mut body: Bytes,
        header_map: HeaderMap,
        _query_parameters: HashMap<String, String>,
        _full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        // If `Content-Encoding` header isn't `snappy` HttpSource won't decode it for us
        // se we need to.
        if header_map
            .get("Content-Encoding")
            .map(|header| header.as_ref())
            != Some(&b"snappy"[..])
        {
            body = decode(&Some("snappy".to_string()), body)?;
        }
        let events = self.decode_body(body)?;
        Ok(events)
    }
}

#[cfg(test)]
mod test {
    use chrono::{SubsecRound as _, Utc};
    use vector_core::event::{EventStatus, Metric, MetricKind, MetricValue};

    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        sinks::prometheus::remote_write::RemoteWriteConfig,
        test_util::{self, components},
        tls::MaybeTlsSettings,
        SourceSender,
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
        receives_metrics(Some(TlsConfig::test_config())).await;
    }

    async fn receives_metrics(tls: Option<TlsConfig>) {
        components::init_test();
        let address = test_util::next_addr();
        let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

        let proto = MaybeTlsSettings::from_config(&tls, true)
            .unwrap()
            .http_protocol_name();
        let source = PrometheusRemoteWriteConfig {
            address,
            auth: None,
            tls: tls.clone(),
            acknowledgements: AcknowledgementsConfig::default(),
        };
        let source = source.build(SourceContext::new_test(tx)).await.unwrap();
        tokio::spawn(source);

        let sink = RemoteWriteConfig {
            endpoint: format!("{}://localhost:{}/", proto, address.port()),
            tls: tls.map(|tls| tls.options),
            ..Default::default()
        };
        let (sink, _) = sink
            .build(SinkContext::new_test())
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
        components::SOURCE_TESTS.assert(&["http_path"]);

        // The MetricBuffer used by the sink may reorder the metrics, so
        // put them back into order before comparing.
        output.sort_unstable_by_key(|event| event.as_metric().name().to_owned());

        shared::assert_event_data_eq!(events, output);
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
                    buckets: vector_core::buckets![ 2.3 => 11, 4.2 => 85 ],
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
                    quantiles: vector_core::quantiles![ 0.1 => 1.2, 0.5 => 3.6, 0.9 => 5.2 ],
                    count: 23,
                    sum: 8.6,
                },
            )
            .with_timestamp(Some(timestamp()))
            .into(),
        ]
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use tokio::time::Duration;

    use super::*;
    use crate::{test_util, test_util::components, SourceSender};

    fn source_receive_address() -> String {
        std::env::var("SOURCE_RECEIVE_ADDRESS").unwrap_or_else(|_| "127.0.0.1:9102".into())
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
        components::init_test();
        let config = PrometheusRemoteWriteConfig {
            address: source_receive_address().parse().unwrap(),
            auth: None,
            tls: None,
            acknowledgements: AcknowledgementsConfig::default(),
        };

        let (tx, rx) = SourceSender::new_with_buffer(4096);
        let source = config.build(SourceContext::new_test(tx)).await.unwrap();
        tokio::spawn(source);

        tokio::time::sleep(Duration::from_secs(2)).await;
        let events = test_util::collect_ready(rx).await;
        assert!(!events.is_empty());

        components::SOURCE_TESTS.assert(&["http_path"]);
    }
}
