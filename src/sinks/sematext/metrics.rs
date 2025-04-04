use std::{collections::HashMap, future::ready, task::Poll};

use bytes::{Bytes, BytesMut};
use futures::{future::BoxFuture, stream, FutureExt, SinkExt};
use http::{StatusCode, Uri};
use hyper::{Body, Request};
use indoc::indoc;
use tower::Service;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::{ByteSizeOf, EstimatedJsonEncodedSizeOf};

use super::Region;
use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::{
        metric::{Metric, MetricValue},
        Event, KeyString,
    },
    http::HttpClient,
    internal_events::{SematextMetricsEncodeEventError, SematextMetricsInvalidMetricError},
    sinks::{
        influxdb::{encode_timestamp, encode_uri, influx_line_protocol, Field, ProtocolVersion},
        util::{
            buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
            http::{HttpBatchService, HttpRetryLogic},
            BatchConfig, EncodedEvent, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, HealthcheckError, VectorSink,
    },
    vector_version, Result,
};

#[derive(Clone)]
struct SematextMetricsService {
    config: SematextMetricsConfig,
    inner: HttpBatchService<BoxFuture<'static, Result<Request<Bytes>>>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SematextMetricsDefaultBatchSettings;

impl SinkBatchSettings for SematextMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `sematext_metrics` sink.
#[configurable_component(sink("sematext_metrics", "Publish metric events to Sematext."))]
#[derive(Clone, Debug)]
pub struct SematextMetricsConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[configurable(metadata(docs::examples = "service"))]
    pub default_namespace: String,

    #[serde(default = "super::default_region")]
    #[configurable(derived)]
    pub region: Region,

    /// The endpoint to send data to.
    ///
    /// Setting this option overrides the `region` option.
    #[configurable(metadata(docs::examples = "http://127.0.0.1"))]
    #[configurable(metadata(docs::examples = "https://example.com"))]
    pub endpoint: Option<String>,

    /// The token that is used to write to Sematext.
    #[configurable(metadata(docs::examples = "${SEMATEXT_TOKEN}"))]
    #[configurable(metadata(docs::examples = "some-sematext-token"))]
    pub token: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    pub(self) batch: BatchConfig<SematextMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for SematextMetricsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            default_namespace = "vector"
            token = "${SEMATEXT_TOKEN}"
        "#})
        .unwrap()
    }
}

async fn healthcheck(endpoint: String, client: HttpClient) -> Result<()> {
    let uri = format!("{}/health", endpoint);

    let request = Request::get(uri)
        .body(Body::empty())
        .map_err(|e| e.to_string())?;

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::NO_CONTENT => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

// https://sematext.com/docs/monitoring/custom-metrics/
const US_ENDPOINT: &str = "https://spm-receiver.sematext.com";
const EU_ENDPOINT: &str = "https://spm-receiver.eu.sematext.com";

#[async_trait::async_trait]
#[typetag::serde(name = "sematext_metrics")]
impl SinkConfig for SematextMetricsConfig {
    async fn build(&self, cx: SinkContext) -> Result<(VectorSink, Healthcheck)> {
        let client = HttpClient::new(None, cx.proxy())?;

        let endpoint = match (&self.endpoint, &self.region) {
            (Some(endpoint), _) => endpoint.clone(),
            (None, Region::Us) => US_ENDPOINT.to_owned(),
            (None, Region::Eu) => EU_ENDPOINT.to_owned(),
        };

        let healthcheck = healthcheck(endpoint.clone(), client.clone()).boxed();
        let sink = SematextMetricsService::new(self.clone(), write_uri(&endpoint)?, client)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

fn write_uri(endpoint: &str) -> Result<Uri> {
    encode_uri(
        endpoint,
        "write",
        &[
            ("db", Some("metrics".into())),
            ("v", Some(format!("vector-{}", vector_version()))),
            ("precision", Some("ns".into())),
        ],
    )
}

impl SematextMetricsService {
    pub fn new(
        config: SematextMetricsConfig,
        endpoint: http::Uri,
        client: HttpClient,
    ) -> Result<VectorSink> {
        let batch = config.batch.into_batch_settings()?;
        let request = config.request.into_settings();
        let http_service = HttpBatchService::new(client, create_build_request(endpoint));
        let sematext_service = SematextMetricsService {
            config,
            inner: http_service,
        };
        let mut normalizer = MetricNormalizer::<SematextMetricNormalize>::default();

        let sink = request
            .batch_sink(
                HttpRetryLogic,
                sematext_service,
                MetricsBuffer::new(batch.size),
                batch.timeout,
            )
            .with_flat_map(move |event: Event| {
                stream::iter({
                    let byte_size = event.size_of();
                    let json_byte_size = event.estimated_json_encoded_size_of();
                    normalizer
                        .normalize(event.into_metric())
                        .map(|item| Ok(EncodedEvent::new(item, byte_size, json_byte_size)))
                })
            })
            .sink_map_err(|error| error!(message = "Fatal sematext metrics sink error.", %error));

        #[allow(deprecated)]
        Ok(VectorSink::from_event_sink(sink))
    }
}

impl Service<Vec<Metric>> for SematextMetricsService {
    type Response = http::Response<bytes::Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, std::result::Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let input = encode_events(
            self.config.token.inner(),
            &self.config.default_namespace,
            items,
        );
        let body = input.item;

        self.inner.call(body)
    }
}

#[derive(Default)]
struct SematextMetricNormalize;

impl MetricNormalize for SematextMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match &metric.value() {
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            MetricValue::Counter { .. } => state.make_incremental(metric),
            _ => {
                emit!(SematextMetricsInvalidMetricError { metric: &metric });
                None
            }
        }
    }
}

fn create_build_request(
    uri: http::Uri,
) -> impl Fn(Bytes) -> BoxFuture<'static, Result<Request<Bytes>>> + Sync + Send + 'static {
    move |body| {
        Box::pin(ready(
            Request::post(uri.clone())
                .header("Content-Type", "text/plain")
                .body(body)
                .map_err(Into::into),
        ))
    }
}

fn encode_events(
    token: &str,
    default_namespace: &str,
    metrics: Vec<Metric>,
) -> EncodedEvent<Bytes> {
    let mut output = BytesMut::new();
    let byte_size = metrics.size_of();
    let json_byte_size = metrics.estimated_json_encoded_size_of();
    for metric in metrics.into_iter() {
        let (series, data, _metadata) = metric.into_parts();
        let namespace = series
            .name
            .namespace
            .unwrap_or_else(|| default_namespace.into());
        let label = series.name.name;
        let ts = encode_timestamp(data.time.timestamp);

        // Authentication in Sematext is by inserting the token as a tag.
        let mut tags = series.tags.unwrap_or_default();
        tags.replace("token".into(), token.to_string());
        let (metric_type, fields) = match data.value {
            MetricValue::Counter { value } => ("counter", to_fields(label, value)),
            MetricValue::Gauge { value } => ("gauge", to_fields(label, value)),
            _ => unreachable!(), // handled by SematextMetricNormalize
        };

        tags.replace("metric_type".into(), metric_type.to_string());

        if let Err(error) = influx_line_protocol(
            ProtocolVersion::V1,
            &namespace,
            Some(tags),
            Some(fields),
            ts,
            &mut output,
        ) {
            emit!(SematextMetricsEncodeEventError { error });
        };
    }

    if !output.is_empty() {
        output.truncate(output.len() - 1);
    }
    EncodedEvent::new(output.freeze(), byte_size, json_byte_size)
}

fn to_fields(label: String, value: f64) -> HashMap<KeyString, Field> {
    let mut result = HashMap::new();
    result.insert(label.into(), Field::Float(value));
    result
}

#[cfg(test)]
mod tests {
    use chrono::{offset::TimeZone, Timelike, Utc};
    use futures::StreamExt;
    use indoc::indoc;
    use vector_lib::metric_tags;

    use super::*;
    use crate::{
        event::{metric::MetricKind, Event},
        sinks::util::test::{build_test_server, load_sink},
        test_util::{
            components::{assert_sink_compliance, HTTP_SINK_TAGS},
            next_addr, test_generate_config,
        },
    };

    #[test]
    fn generate_config() {
        test_generate_config::<SematextMetricsConfig>();
    }

    #[test]
    fn test_encode_counter_event() {
        let events = vec![Metric::new(
            "pool.used",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
        )
        .with_namespace(Some("jvm"))
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2020, 8, 18, 21, 0, 0)
                .single()
                .expect("invalid timestamp"),
        ))];

        assert_eq!(
            "jvm,metric_type=counter,token=aaa pool.used=42 1597784400000000000",
            encode_events("aaa", "ns", events).item
        );
    }

    #[test]
    fn test_encode_counter_event_no_namespace() {
        let events = vec![Metric::new(
            "used",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
        )
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2020, 8, 18, 21, 0, 0)
                .single()
                .expect("invalid timestamp"),
        ))];

        assert_eq!(
            "ns,metric_type=counter,token=aaa used=42 1597784400000000000",
            encode_events("aaa", "ns", events).item
        );
    }

    #[test]
    fn test_encode_counter_multiple_events() {
        let events = vec![
            Metric::new(
                "pool.used",
                MetricKind::Incremental,
                MetricValue::Counter { value: 42.0 },
            )
            .with_namespace(Some("jvm"))
            .with_timestamp(Some(
                Utc.with_ymd_and_hms(2020, 8, 18, 21, 0, 0)
                    .single()
                    .expect("invalid timestamp"),
            )),
            Metric::new(
                "pool.committed",
                MetricKind::Incremental,
                MetricValue::Counter { value: 18874368.0 },
            )
            .with_namespace(Some("jvm"))
            .with_timestamp(Some(
                Utc.with_ymd_and_hms(2020, 8, 18, 21, 0, 0)
                    .single()
                    .and_then(|t| t.with_nanosecond(1))
                    .expect("invalid timestamp"),
            )),
        ];

        assert_eq!(
            "jvm,metric_type=counter,token=aaa pool.used=42 1597784400000000000\n\
             jvm,metric_type=counter,token=aaa pool.committed=18874368 1597784400000000001",
            encode_events("aaa", "ns", events).item
        );
    }

    #[tokio::test]
    async fn smoke() {
        assert_sink_compliance(&HTTP_SINK_TAGS, async {

        let (mut config, cx) = load_sink::<SematextMetricsConfig>(indoc! {r#"
            default_namespace = "ns"
            token = "atoken"
            batch.max_events = 1
        "#})
        .unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        // Make our test metrics.
        let metrics = vec![
            ("os", "swap.size", 324292.0),
            ("os", "network.tx", 42000.0),
            ("os", "network.rx", 54293.0),
            ("process", "count", 12.0),
            ("process", "uptime", 32423.0),
            ("process", "rss", 2342333.0),
            ("jvm", "pool.used", 18874368.0),
            ("jvm", "pool.committed", 18868584.0),
            ("jvm", "pool.max", 18874368.0),
        ];

        let mut events = Vec::new();
        for (i, (namespace, metric, val)) in metrics.iter().enumerate() {
            let event = Event::from(
                Metric::new(
                    *metric,
                    MetricKind::Incremental,
                    MetricValue::Counter { value: *val },
                )
                .with_namespace(Some(*namespace))
                .with_tags(Some(metric_tags!("os.host" => "somehost")))
                    .with_timestamp(Some(Utc.with_ymd_and_hms(2020, 8, 18, 21, 0, 0).single()
                                         .and_then(|t| t.with_nanosecond(i as u32))
                                         .expect("invalid timestamp"))),
            );
            events.push(event);
        }

        sink.run_events(events).await.unwrap();

        let output = rx.take(metrics.len()).collect::<Vec<_>>().await;
        assert_eq!("os,metric_type=counter,os.host=somehost,token=atoken swap.size=324292 1597784400000000000", output[0].1);
        assert_eq!("os,metric_type=counter,os.host=somehost,token=atoken network.tx=42000 1597784400000000001", output[1].1);
        assert_eq!("os,metric_type=counter,os.host=somehost,token=atoken network.rx=54293 1597784400000000002", output[2].1);
        assert_eq!("process,metric_type=counter,os.host=somehost,token=atoken count=12 1597784400000000003", output[3].1);
        assert_eq!("process,metric_type=counter,os.host=somehost,token=atoken uptime=32423 1597784400000000004", output[4].1);
        assert_eq!("process,metric_type=counter,os.host=somehost,token=atoken rss=2342333 1597784400000000005", output[5].1);
        assert_eq!("jvm,metric_type=counter,os.host=somehost,token=atoken pool.used=18874368 1597784400000000006", output[6].1);
        assert_eq!("jvm,metric_type=counter,os.host=somehost,token=atoken pool.committed=18868584 1597784400000000007", output[7].1);
        assert_eq!("jvm,metric_type=counter,os.host=somehost,token=atoken pool.max=18874368 1597784400000000008", output[8].1);
        }).await;
    }
}
