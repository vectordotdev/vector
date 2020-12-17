use crate::{
    config::{self, GenerateConfig, GlobalOptions, SourceConfig, SourceDescription},
    event::{Metric, MetricKind, MetricValue},
    internal_events::{
        PrometheusNoNameError, PrometheusRemoteWriteParseError, PrometheusRemoteWriteReceived,
        PrometheusRemoteWriteSnapError,
    },
    prometheus::{proto, METRIC_NAME_LABEL},
    shutdown::ShutdownSignal,
    sources::{
        self,
        util::{ErrorMessage, HttpSource, HttpSourceAuthConfig},
    },
    tls::TlsConfig,
    Event, Pipeline,
};
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
};
use warp::http::{HeaderMap, StatusCode};

const SOURCE_NAME: &str = "prometheus_remote_write";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PrometheusRemoteWriteConfig {
    address: SocketAddr,

    tls: Option<TlsConfig>,

    auth: Option<HttpSourceAuthConfig>,
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
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SourceConfig for PrometheusRemoteWriteConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<sources::Source> {
        let source = RemoteWriteSource {
            decompressor: snap::raw::Decoder::new(),
        };
        source.run(self.address, "", &self.tls, &self.auth, out, shutdown)
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        SOURCE_NAME
    }
}

#[derive(Clone)]
struct RemoteWriteSource {
    decompressor: snap::raw::Decoder,
}

impl RemoteWriteSource {
    fn decode_body(&self, body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        let body = self
            .decompressor
            .clone()
            .decompress_vec(&body)
            .map_err(|error| {
                emit!(PrometheusRemoteWriteSnapError {
                    error: error.clone()
                });
                ErrorMessage::new(
                    StatusCode::BAD_REQUEST,
                    format!("Could not decompress write request: {}", error),
                )
            })?;
        let request = proto::WriteRequest::decode(Bytes::from(body)).map_err(|error| {
            emit!(PrometheusRemoteWriteParseError {
                error: error.clone()
            });
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {}", error),
            )
        })?;
        Ok(decode_request(request))
    }
}

impl HttpSource for RemoteWriteSource {
    fn build_event(
        &self,
        body: Bytes,
        _header_map: HeaderMap,
        _query_parameters: HashMap<String, String>,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let byte_size = body.len();
        let result = self.decode_body(body)?;
        let count = result.len();
        emit!(PrometheusRemoteWriteReceived { byte_size, count });
        Ok(result)
    }
}

fn decode_request(request: proto::WriteRequest) -> Vec<Event> {
    request
        .timeseries
        .into_iter()
        .filter_map(decode_timeseries)
        .flatten()
        .collect()
}

fn decode_timeseries(timeseries: proto::TimeSeries) -> Option<impl Iterator<Item = Event>> {
    let (name, tags) = parse_labels(timeseries.labels);
    match name {
        Some(name) => Some(timeseries.samples.into_iter().map(move |sample| {
            let value = sample.value;
            let value = if name.ends_with("_total") {
                MetricValue::Counter { value }
            } else {
                MetricValue::Gauge { value }
            };
            Metric {
                name: name.clone(),
                namespace: None,
                timestamp: parse_timestamp(sample.timestamp),
                tags: tags.clone(),
                kind: MetricKind::Absolute,
                value,
            }
            .into()
        })),
        None => {
            emit!(PrometheusNoNameError);
            None
        }
    }
}

fn parse_labels(labels: Vec<proto::Label>) -> (Option<String>, Option<BTreeMap<String, String>>) {
    let mut tags = labels
        .into_iter()
        .map(|label| (label.name, label.value))
        .collect::<BTreeMap<String, String>>();
    let name = tags.remove(METRIC_NAME_LABEL);
    let tags = if tags.is_empty() { None } else { Some(tags) };
    (name, tags)
}

fn parse_timestamp(timestamp: i64) -> Option<DateTime<Utc>> {
    // Conversion into UTC should never produce an ambiguous time, but
    // we still need to pick one so arbitrarily choose the latest.
    Utc.timestamp_opt(timestamp / 1000, (timestamp % 1000) as u32 * 1000000)
        .latest()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        event::{MetricKind, MetricValue},
        sinks::prometheus::remote_write::RemoteWriteConfig,
        test_util, Pipeline,
    };
    use chrono::{SubsecRound as _, Utc};
    use futures::stream;

    #[test]
    fn genreate_config() {
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
        let address = test_util::next_addr();
        let (tx, rx) = Pipeline::new_test();

        let proto = if tls.is_none() { "http" } else { "https" };
        let source = PrometheusRemoteWriteConfig {
            address,
            auth: None,
            tls: tls.clone(),
        };
        let source = source
            .build(
                "source",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap();
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
        sink.run(stream::iter(events.clone())).await.unwrap();

        let mut output = test_util::collect_ready(rx).await;
        // The MetricBuffer used by the sink may reorder the metrics, so
        // put them back into order before comparing.
        output.sort_unstable_by_key(|event| event.as_metric().name.clone());

        assert_eq!(events, output);
    }

    fn make_events() -> Vec<Event> {
        (0..10)
            .map(|num| {
                let timestamp = Utc::now().trunc_subsecs(3);
                Event::Metric(Metric {
                    name: format!("gauge_{}", num),
                    namespace: None,
                    timestamp: Some(timestamp),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: num as f64 },
                })
            })
            .collect()
    }
}
