use std::{collections::HashMap, fmt, future::ready, mem, sync::Arc, time::Duration};

use bytes::{Bytes, BytesMut};
use futures::{StreamExt, stream::BoxStream};
use http_1::Uri;
use indoc::indoc;
use tower::{Service, ServiceBuilder};
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    configurable::configurable_component,
    event::metric::{MetricSketch, MetricTags, Quantile},
    finalization::{EventFinalizers, EventStatus},
    internal_event::CountByteSize,
    json_size::JsonSize,
    request_metadata::RequestMetadata,
    sensitive_string::SensitiveString,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse, batcher::BatchConfig as StreamBatchConfig},
};

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::{
        Event, KeyString,
        metric::{Metric, MetricValue, Sample, StatisticKind},
    },
    http::client_v1::HttpClient,
    internal_events::InfluxdbEncodingError,
    sinks::{
        Healthcheck, VectorSink,
        influxdb::{
            Field, InfluxDbSettings, InfluxDbWriteRequest, ProtocolVersion, encode_timestamp,
            healthcheck, influx_line_protocol, influxdb_settings,
        },
        util::{
            BatchConfig, HttpEndpoint, SinkBatchSettings, TowerRequestConfig,
            buffer::metrics::{
                MetricNormalize, MetricNormalizer, MetricSet, MetricSetSettings,
                compress_distribution,
            },
            builder::SinkBuilderExt,
            encode_namespace,
            http::{HttpRequest, RetryStrategy},
            http_v1::{HttpService, HttpServiceRequestBuilder, http_response_retry_logic},
            service::ServiceBuilderExt,
            statistic::{DistributionStatistic, validate_quantiles},
        },
    },
    tls::{TlsConfig, TlsSettings},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct InfluxDbDefaultBatchSettings;

impl SinkBatchSettings for InfluxDbDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `influxdb_metrics` sink.
#[configurable_component(sink("influxdb_metrics", "Deliver metric event data to InfluxDB."))]
#[derive(Clone, Debug)]
pub struct InfluxDbConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[serde(alias = "namespace")]
    #[configurable(metadata(docs::examples = "service"))]
    pub default_namespace: Option<String>,

    /// The endpoint to send data to.
    ///
    /// This should be a full HTTP URI, including the scheme, host, and port.
    #[configurable(metadata(docs::examples = "http://localhost:8086/"))]
    pub endpoint: HttpEndpoint,

    #[serde(flatten)]
    pub settings: InfluxDbSettings,

    #[serde(default)]
    pub batch: BatchConfig<InfluxDbDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    /// A map of additional tags, in the key/value pair format, to add to each measurement.
    #[configurable(metadata(docs::additional_props_description = "A tag key/value pair."))]
    #[configurable(metadata(docs::examples = "example_tags()"))]
    pub tags: Option<HashMap<String, String>>,

    pub tls: Option<TlsConfig>,

    /// The list of quantiles to calculate when sending distribution metrics.
    #[serde(default = "default_summary_quantiles")]
    pub quantiles: Vec<f64>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

#[must_use]
pub fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}

#[must_use]
pub fn example_tags() -> HashMap<String, String> {
    HashMap::from([("region".to_string(), "us-west-1".to_string())])
}

impl GenerateConfig for InfluxDbConfig {
    fn generate_config() -> serde_json::Value {
        toml::from_str(indoc! {r#"
            endpoint = "http://localhost:8086/"
            version = "2"
            org = "my-org"
            bucket = "my-bucket"
            token = "${INFLUXDB_TOKEN}"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "influxdb_metrics")]
impl SinkConfig for InfluxDbConfig {
    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, derive_more::Debug)]
pub struct ValidatedInfluxDbMetrics {
    // Omitted: the retained `uri` embeds the v1 password in its `p` query parameter.
    #[debug(skip)]
    uri: Uri,
    token: SensitiveString,
    protocol_version: ProtocolVersion,
    batch: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for InfluxDbConfig {
    type Validated = ValidatedInfluxDbMetrics;

    fn validate(&self) -> crate::Result<ValidatedInfluxDbMetrics> {
        validate_quantiles(&self.quantiles)?;

        let settings = influxdb_settings(self.settings.clone());

        let uri = settings.write_uri(self.endpoint.clone())?;

        let token = settings.token();
        let protocol_version = settings.protocol_version();

        let batch = self
            .batch
            .validate()?
            .disallow_max_bytes()?
            .into_batcher_settings()?;

        Ok(ValidatedInfluxDbMetrics {
            uri,
            token,
            protocol_version,
            batch,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedInfluxDbMetrics,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedInfluxDbMetrics {
            uri,
            token,
            protocol_version,
            batch,
        } = validated;

        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings.into(), cx.proxy())?;
        let healthcheck =
            healthcheck(self.endpoint.clone(), self.settings.clone(), client.clone())?;

        let request_builder = InfluxDbMetricsRequestBuilder {
            write: InfluxDbWriteRequest::new(uri.clone(), token),
            protocol_version: *protocol_version,
            default_namespace: self.default_namespace.clone(),
            tags: self.tags.clone(),
            quantiles: self.quantiles.clone(),
        };
        let service = HttpService::new(client, request_builder).with_response_decompression();
        let service = ServiceBuilder::new()
            .settings(
                self.request.into_settings(),
                http_response_retry_logic(RetryStrategy::Default),
            )
            .service(service);

        let sink = InfluxDbMetricsSink {
            service,
            batch_settings: *batch,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

/// A normalized metric with the accounting of the event it came from.
struct NormalizedMetric {
    metric: Metric,
    byte_size: usize,
    json_size: JsonSize,
}

/// The aggregated metrics of one write request, with the accounting of every event added to it.
struct MetricBatch {
    metrics: Vec<Metric>,
    count: usize,
    byte_size: usize,
    json_size: JsonSize,
}

fn metric_set(max_events: usize) -> MetricSet {
    MetricSet::new(MetricSetSettings {
        max_events: Some(max_events),
        ..Default::default()
    })
}

/// Aggregates metrics by series until a batch holds `max_events` series.
struct MetricBatchConfig {
    metrics: MetricSet,
    count: usize,
    byte_size: usize,
    json_size: JsonSize,
    settings: BatcherSettings,
}

impl MetricBatchConfig {
    fn new(settings: BatcherSettings) -> Self {
        Self {
            metrics: metric_set(settings.item_limit),
            count: 0,
            byte_size: 0,
            json_size: JsonSize::zero(),
            settings,
        }
    }
}

impl StreamBatchConfig<NormalizedMetric> for MetricBatchConfig {
    type ItemMetadata = ();
    type Batch = MetricBatch;

    fn len(&self) -> usize {
        self.count
    }

    fn take_batch(&mut self) -> MetricBatch {
        let metrics = mem::replace(&mut self.metrics, metric_set(self.settings.item_limit));
        let mut metrics = metrics.into_metrics();
        for metric in &mut metrics {
            if let MetricValue::Distribution { samples, .. } = metric.data_mut().value_mut() {
                *samples = compress_distribution(samples);
            }
        }
        MetricBatch {
            metrics,
            count: mem::take(&mut self.count),
            byte_size: mem::take(&mut self.byte_size),
            json_size: mem::take(&mut self.json_size),
        }
    }

    fn push(&mut self, item: NormalizedMetric, (): ()) {
        self.count += 1;
        self.byte_size += item.byte_size;
        self.json_size += item.json_size;
        self.metrics.insert_update(item.metric);
    }

    fn is_batch_full(&self) -> bool {
        self.metrics.len() >= self.settings.item_limit
    }

    fn item_fits_in_batch(&self, _item: &NormalizedMetric) -> (bool, ()) {
        // A batch is taken as soon as it is full, so every metric fits.
        (true, ())
    }

    fn timeout(&self) -> Duration {
        self.settings.timeout
    }
}

/// Encodes the batched metrics of each request attempt.
///
/// Every attempt, retries included, encodes the metrics again: metrics without a timestamp are
/// written with the time of the attempt.
#[derive(Clone)]
struct InfluxDbMetricsRequestBuilder {
    write: InfluxDbWriteRequest,
    protocol_version: ProtocolVersion,
    default_namespace: Option<String>,
    tags: Option<HashMap<String, String>>,
    quantiles: Vec<f64>,
}

impl HttpServiceRequestBuilder<Arc<MetricBatch>> for InfluxDbMetricsRequestBuilder {
    fn build(
        &self,
        request: HttpRequest<Arc<MetricBatch>>,
    ) -> crate::Result<http_1::Request<Bytes>> {
        let body = encode_events(
            self.protocol_version,
            &request.get_additional_metadata().metrics,
            self.default_namespace.as_deref(),
            self.tags.as_ref(),
            &self.quantiles,
        );
        self.write.request(body.freeze())
    }
}

/// Creates the request for a batch.
///
/// The metrics keep their finalizers, so the driver does not report the response status to
/// them. They are released without a status once the request and all of its retries complete.
fn build_request(batch: MetricBatch) -> HttpRequest<Arc<MetricBatch>> {
    // The body is only encoded by each attempt, so its size is unknown here. The driver does not
    // report byte telemetry for this sink, and `EndpointBytesSent` uses the actual body size.
    let metadata = RequestMetadata::new(
        batch.count,
        batch.byte_size,
        0,
        0,
        // Events are reported untagged, with the estimated JSON size of the original events.
        CountByteSize(batch.count, batch.json_size).into(),
    );
    HttpRequest::new(
        Bytes::new(),
        EventFinalizers::default(),
        metadata,
        Arc::new(batch),
    )
}

struct InfluxDbMetricsSink<S> {
    service: S,
    batch_settings: BatcherSettings,
}

impl<S> InfluxDbMetricsSink<S>
where
    S: Service<HttpRequest<Arc<MetricBatch>>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let Self {
            service,
            batch_settings,
        } = *self;
        let mut normalizer = MetricNormalizer::<InfluxMetricNormalize>::default();

        input
            .filter_map(move |event| {
                // The accounting of the original event is kept through normalization.
                let byte_size = event.size_of();
                let json_size = event.estimated_json_encoded_size_of();
                ready(
                    normalizer
                        .normalize(event.into_metric())
                        .map(|metric| NormalizedMetric {
                            metric,
                            byte_size,
                            json_size,
                        }),
                )
            })
            .batched(MetricBatchConfig::new(batch_settings))
            .map(build_request)
            .into_driver(service)
            .with_request_error_status(EventStatus::Errored)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for InfluxDbMetricsSink<S>
where
    S: Service<HttpRequest<Arc<MetricBatch>>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

fn merge_tags(event: &Metric, tags: Option<&HashMap<String, String>>) -> Option<MetricTags> {
    match (event.tags().cloned(), tags) {
        (Some(mut event_tags), Some(config_tags)) => {
            event_tags.extend(config_tags.iter().map(|(k, v)| (k.clone(), v.clone())));
            Some(event_tags)
        }
        (Some(event_tags), None) => Some(event_tags),
        (None, Some(config_tags)) => Some(
            config_tags
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        (None, None) => None,
    }
}

#[derive(Default)]
pub struct InfluxMetricNormalize;

impl MetricNormalize for InfluxMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match (metric.kind(), &metric.value()) {
            // Counters are disaggregated. We take the previous value from the state
            // and emit the difference between previous and current as a Counter
            (_, MetricValue::Counter { .. }) => state.make_incremental(metric),
            // Convert incremental gauges into absolute ones
            (_, MetricValue::Gauge { .. }) => state.make_absolute(metric),
            // All others are left as-is
            _ => Some(metric),
        }
    }
}

fn encode_events(
    protocol_version: ProtocolVersion,
    events: &[Metric],
    default_namespace: Option<&str>,
    tags: Option<&HashMap<String, String>>,
    quantiles: &[f64],
) -> BytesMut {
    let mut output = BytesMut::new();
    let count = events.len();

    for event in events {
        let fullname = encode_namespace(event.namespace().or(default_namespace), '.', event.name());
        let ts = encode_timestamp(event.timestamp());
        let tags = merge_tags(event, tags);
        let (metric_type, fields) = get_type_and_fields(event.value(), quantiles);

        let mut unwrapped_tags = tags.unwrap_or_default();
        unwrapped_tags.replace("metric_type".to_owned(), metric_type.to_owned());

        if let Err(error_message) = influx_line_protocol(
            protocol_version,
            &fullname,
            Some(unwrapped_tags),
            fields,
            ts,
            &mut output,
        ) {
            emit!(InfluxdbEncodingError {
                error_message,
                count,
            });
        }
    }

    // remove last '\n'
    if !output.is_empty() {
        output.truncate(output.len() - 1);
    }
    output
}

fn get_type_and_fields(
    value: &MetricValue,
    quantiles: &[f64],
) -> (&'static str, Option<HashMap<KeyString, Field>>) {
    match value {
        MetricValue::Counter { value } => ("counter", Some(to_fields(*value))),
        MetricValue::Gauge { value } => ("gauge", Some(to_fields(*value))),
        MetricValue::Set { values } => {
            // https://github.com/vectordotdev/vector/issues/23659
            #[allow(clippy::cast_precision_loss, reason = "Preserve set-size rounding.")]
            let count = values.len() as f64;
            ("set", Some(to_fields(count)))
        }
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            let mut fields: HashMap<KeyString, Field> = buckets
                .iter()
                .map(|sample| {
                    (
                        format!("bucket_{}", sample.upper_limit).into(),
                        Field::UnsignedInt(sample.count),
                    )
                })
                .collect();
            fields.insert("count".into(), Field::UnsignedInt(*count));
            fields.insert("sum".into(), Field::Float(*sum));

            ("histogram", Some(fields))
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            let mut fields: HashMap<KeyString, Field> = quantiles
                .iter()
                .map(|quantile| {
                    (
                        format!("quantile_{}", quantile.quantile).into(),
                        Field::Float(quantile.value),
                    )
                })
                .collect();
            fields.insert("count".into(), Field::UnsignedInt(*count));
            fields.insert("sum".into(), Field::Float(*sum));

            ("summary", Some(fields))
        }
        MetricValue::Distribution { samples, statistic } => {
            let quantiles = match statistic {
                StatisticKind::Histogram => &[0.95] as &[_],
                StatisticKind::Summary => quantiles,
            };
            let fields = encode_distribution(samples, quantiles);
            ("distribution", fields)
        }
        MetricValue::Sketch { sketch } => match sketch {
            MetricSketch::AgentDDSketch(ddsketch) => {
                // Hard-coded quantiles because InfluxDB can't natively do anything useful with the
                // actual bins.
                let mut fields = [0.5, 0.75, 0.9, 0.99]
                    .iter()
                    .map(|q| {
                        let quantile = Quantile {
                            quantile: *q,
                            value: ddsketch.quantile(*q).unwrap_or(0.0),
                        };
                        (
                            quantile.to_percentile_string().into(),
                            Field::Float(quantile.value),
                        )
                    })
                    .collect::<HashMap<KeyString, _>>();
                fields.insert(
                    "count".into(),
                    Field::UnsignedInt(u64::from(ddsketch.count())),
                );
                fields.insert(
                    "min".into(),
                    Field::Float(ddsketch.min().unwrap_or(f64::MAX)),
                );
                fields.insert(
                    "max".into(),
                    Field::Float(ddsketch.max().unwrap_or(f64::MIN)),
                );
                fields.insert("sum".into(), Field::Float(ddsketch.sum().unwrap_or(0.0)));
                fields.insert("avg".into(), Field::Float(ddsketch.avg().unwrap_or(0.0)));

                ("sketch", Some(fields))
            }
        },
    }
}

fn encode_distribution(samples: &[Sample], quantiles: &[f64]) -> Option<HashMap<KeyString, Field>> {
    let statistic = DistributionStatistic::from_samples(samples, quantiles)?;
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(clippy::cast_precision_loss, reason = "Preserve count rounding.")]
    let count = statistic.count as f64;

    Some(
        [
            ("min".into(), Field::Float(statistic.min)),
            ("max".into(), Field::Float(statistic.max)),
            ("median".into(), Field::Float(statistic.median)),
            ("avg".into(), Field::Float(statistic.avg)),
            ("sum".into(), Field::Float(statistic.sum)),
            ("count".into(), Field::Float(count)),
        ]
        .into_iter()
        .chain(
            statistic
                .quantiles
                .iter()
                .map(|&(p, val)| (format!("quantile_{p:.2}").into(), Field::Float(val))),
        )
        .collect(),
    )
}

fn to_fields(value: f64) -> HashMap<KeyString, Field> {
    [("value".into(), Field::Float(value))]
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use futures::stream;
    use indoc::indoc;
    use similar_asserts::assert_eq;
    use vector_lib::{
        event::{BatchNotifier, BatchStatus},
        finalization::Finalizable,
    };

    use super::*;
    use crate::{
        config::ValidatedSink,
        event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
        sinks::influxdb::{
            InfluxDb2Settings,
            test_util::{assert_fields, split_line_protocol, tags, ts},
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<InfluxDbConfig>();
    }

    #[test]
    fn test_config_with_tags() {
        let config = indoc! {r#"
            namespace: "vector"
            endpoint: "http://localhost:9999"
            version: "2"
            bucket: "my-bucket"
            org: "my-org"
            token: "my-token"
            tags:
              region: "us-west-1"
        "#};

        serde_yaml::from_str::<InfluxDbConfig>(config).unwrap();
    }

    #[test]
    fn test_config_version_selects_settings() {
        let config = indoc! {r#"
            endpoint: "http://localhost:8086"
            version: "1"
            database: "my-database"
        "#};
        let config = serde_yaml::from_str::<InfluxDbConfig>(config).unwrap();
        assert!(matches!(config.settings, InfluxDbSettings::V1(_)));

        let config = indoc! {r#"
            endpoint: "http://localhost:9999"
            version: "2"
            org: "my-org"
            bucket: "my-bucket"
            token: "my-token"
        "#};
        let config = serde_yaml::from_str::<InfluxDbConfig>(config).unwrap();
        assert!(matches!(config.settings, InfluxDbSettings::V2(_)));
    }

    #[test]
    fn test_config_rejects_invalid_settings() {
        let v2 = indoc! {r#"
            endpoint: "http://localhost:9999"
            org: "my-org"
            bucket: "my-bucket"
            token: "my-token"
        "#};
        for (config, expected) in [
            (v2.to_string(), "missing field `version`"),
            (
                format!("version: \"2\"\n{v2}database: my-database\n"),
                "unknown field `database`",
            ),
            (
                format!("version: \"2\"\n{v2}database: null\n"),
                "unknown field `database`",
            ),
            (
                format!("version: \"2\"\n{v2}endpont: http://localhost:9999\n"),
                "unknown field `endpont`",
            ),
        ] {
            let error = serde_yaml::from_str::<InfluxDbConfig>(&config).unwrap_err();
            assert!(error.to_string().contains(expected), "{expected}: {error}");
        }

        // Use the regular config loading path, where an unquoted integer could otherwise be read
        // as a variant index.
        let config = format!("version: 1\n{v2}");
        let errors = crate::config::loading::load::<_, InfluxDbConfig>(
            config.as_bytes(),
            crate::config::Format::Yaml,
        )
        .unwrap_err();
        let error = errors.join("\n");
        assert!(error.contains("invalid type: integer"), "{error}");
    }

    #[test]
    fn prepares_valid_config() {
        let config = InfluxDbConfig {
            endpoint: HttpEndpoint::parse("http://localhost:9999").unwrap(),
            settings: InfluxDbSettings::V2(InfluxDb2Settings {
                org: "my-org".to_string(),
                bucket: "my-bucket".to_string(),
                token: "my-token".to_string().into(),
            }),
            default_namespace: None,
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tags: None,
            tls: None,
            quantiles: default_summary_quantiles(),
            acknowledgements: AcknowledgementsConfig::default(),
        };

        let validated = config.validate().expect("preparation should succeed");
        assert!(matches!(validated.protocol_version, ProtocolVersion::V2));
        assert_eq!(
            validated.uri.to_string(),
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns"
        );
    }

    #[test]
    fn test_encode_counter() {
        let events = vec![
            Metric::new(
                "total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.5 },
            )
            .with_namespace(Some("ns"))
            .with_timestamp(Some(ts())),
            Metric::new(
                "check",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, Some("vector"), None, &[]);
        assert_eq!(
            line_protocols,
            "ns.total,metric_type=counter value=1.5 1542182950000000011\n\
            ns.check,metric_type=counter,normal_tag=value,true_tag=true value=1 1542182950000000011"
        );
    }

    #[test]
    fn test_encode_gauge() {
        let events = vec![
            Metric::new(
                "meter",
                MetricKind::Incremental,
                MetricValue::Gauge { value: -1.5 },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        assert_eq!(
            line_protocols,
            "ns.meter,metric_type=gauge,normal_tag=value,true_tag=true value=-1.5 1542182950000000011"
        );
    }

    #[test]
    fn test_encode_set() {
        let events = vec![
            Metric::new(
                "users",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["alice".into(), "bob".into()].into_iter().collect(),
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        assert_eq!(
            line_protocols,
            "ns.users,metric_type=set,normal_tag=value,true_tag=true value=2 1542182950000000011"
        );
    }

    #[test]
    fn test_encode_histogram_v1() {
        let events = vec![
            Metric::new(
                "requests",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vector_lib::buckets![1.0 => 1, 2.1 => 2, 3.0 => 3],
                    count: 6,
                    sum: 12.5,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V1, &events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let first_record = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", first_record.0);
        assert_eq!(
            "metric_type=histogram,normal_tag=value,true_tag=true",
            first_record.1
        );
        assert_fields(
            &first_record.2,
            [
                "bucket_1=1i",
                "bucket_2.1=2i",
                "bucket_3=3i",
                "count=6i",
                "sum=12.5",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", first_record.3);
    }

    #[test]
    fn test_encode_histogram() {
        let events = vec![
            Metric::new(
                "requests",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vector_lib::buckets![1.0 => 1, 2.1 => 2, 3.0 => 3],
                    count: 6,
                    sum: 12.5,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let first_record = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", first_record.0);
        assert_eq!(
            "metric_type=histogram,normal_tag=value,true_tag=true",
            first_record.1
        );
        assert_fields(
            &first_record.2,
            [
                "bucket_1=1u",
                "bucket_2.1=2u",
                "bucket_3=3u",
                "count=6u",
                "sum=12.5",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", first_record.3);
    }

    #[test]
    fn test_encode_summary_v1() {
        let events = vec![
            Metric::new(
                "requests_sum",
                MetricKind::Absolute,
                MetricValue::AggregatedSummary {
                    quantiles: vector_lib::quantiles![0.01 => 1.5, 0.5 => 2.0, 0.99 => 3.0],
                    count: 6,
                    sum: 12.0,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V1, &events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let first_record = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests_sum", first_record.0);
        assert_eq!(
            "metric_type=summary,normal_tag=value,true_tag=true",
            first_record.1
        );
        assert_fields(
            &first_record.2,
            [
                "count=6i",
                "quantile_0.01=1.5",
                "quantile_0.5=2",
                "quantile_0.99=3",
                "sum=12",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", first_record.3);
    }

    #[test]
    fn test_encode_summary() {
        let events = vec![
            Metric::new(
                "requests_sum",
                MetricKind::Absolute,
                MetricValue::AggregatedSummary {
                    quantiles: vector_lib::quantiles![0.01 => 1.5, 0.5 => 2.0, 0.99 => 3.0],
                    count: 6,
                    sum: 12.0,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let first_record = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests_sum", first_record.0);
        assert_eq!(
            "metric_type=summary,normal_tag=value,true_tag=true",
            first_record.1
        );
        assert_fields(
            &first_record.2,
            [
                "count=6u",
                "quantile_0.01=1.5",
                "quantile_0.5=2",
                "quantile_0.99=3",
                "sum=12",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", first_record.3);
    }

    #[test]
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the complete encoding fixture together."
    )]
    fn test_encode_distribution() {
        let events = vec![
            Metric::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![1.0 => 3, 2.0 => 3, 3.0 => 2],
                    statistic: StatisticKind::Histogram,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
            Metric::new(
                "dense_stats",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: (0..20)
                        .map(|v| Sample {
                            value: f64::from(v),
                            rate: 1,
                        })
                        .collect(),
                    statistic: StatisticKind::Histogram,
                },
            )
            .with_namespace(Some("ns"))
            .with_timestamp(Some(ts())),
            Metric::new(
                "sparse_stats",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: (1..5)
                        .map(|v| Sample {
                            value: f64::from(v),
                            rate: v,
                        })
                        .collect(),
                    statistic: StatisticKind::Histogram,
                },
            )
            .with_namespace(Some("ns"))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 3);

        let first_record = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", first_record.0);
        assert_eq!(
            "metric_type=distribution,normal_tag=value,true_tag=true",
            first_record.1
        );
        assert_fields(
            &first_record.2,
            [
                "avg=1.875",
                "count=8",
                "max=3",
                "median=2",
                "min=1",
                "quantile_0.95=3",
                "sum=15",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", first_record.3);

        let second_record = split_line_protocol(line_protocols[1]);
        assert_eq!("ns.dense_stats", second_record.0);
        assert_eq!("metric_type=distribution", second_record.1);
        assert_fields(
            &second_record.2,
            [
                "avg=9.5",
                "count=20",
                "max=19",
                "median=9",
                "min=0",
                "quantile_0.95=18",
                "sum=190",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", second_record.3);

        let third_record = split_line_protocol(line_protocols[2]);
        assert_eq!("ns.sparse_stats", third_record.0);
        assert_eq!("metric_type=distribution", third_record.1);
        assert_fields(
            &third_record.2,
            [
                "avg=3",
                "count=10",
                "max=4",
                "median=3",
                "min=1",
                "quantile_0.95=4",
                "sum=30",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", third_record.3);
    }

    #[test]
    fn test_encode_distribution_empty_stats() {
        let events = vec![
            Metric::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vec![],
                    statistic: StatisticKind::Histogram,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        assert_eq!(line_protocols.len(), 0);
    }

    #[test]
    fn test_encode_distribution_zero_counts_stats() {
        let events = vec![
            Metric::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![1.0 => 0, 2.0 => 0],
                    statistic: StatisticKind::Histogram,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(ProtocolVersion::V2, &events, None, None, &[]);
        assert_eq!(line_protocols.len(), 0);
    }

    #[test]
    fn test_encode_distribution_summary() {
        let events = vec![
            Metric::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![1.0 => 3, 2.0 => 3, 3.0 => 2],
                    statistic: StatisticKind::Summary,
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let line_protocols = encode_events(
            ProtocolVersion::V2,
            &events,
            None,
            None,
            &default_summary_quantiles(),
        );
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol.0);
        assert_eq!(
            "metric_type=distribution,normal_tag=value,true_tag=true",
            line_protocol.1
        );
        assert_fields(
            &line_protocol.2,
            [
                "avg=1.875",
                "count=8",
                "max=3",
                "median=2",
                "min=1",
                "sum=15",
                "quantile_0.50=2",
                "quantile_0.75=2",
                "quantile_0.90=3",
                "quantile_0.95=3",
                "quantile_0.99=3",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol.3);
    }

    #[test]
    fn test_encode_with_some_tags() {
        crate::test_util::trace_init();

        let events = vec![
            Metric::new(
                "cpu",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 2.5 },
            )
            .with_namespace(Some("vector"))
            .with_timestamp(Some(ts())),
            Metric::new(
                "mem",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1000.0 },
            )
            .with_namespace(Some("vector"))
            .with_tags(Some(tags()))
            .with_timestamp(Some(ts())),
        ];

        let mut tags = HashMap::new();
        tags.insert("host".to_owned(), "local".to_owned());
        tags.insert("datacenter".to_owned(), "us-east".to_owned());

        let line_protocols = encode_events(
            ProtocolVersion::V1,
            &events,
            Some("ns"),
            Some(tags).as_ref(),
            &[],
        );
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 2);
        assert_eq!(
            line_protocols[0],
            "vector.cpu,datacenter=us-east,host=local,metric_type=gauge value=2.5 1542182950000000011"
        );
        assert_eq!(
            line_protocols[1],
            "vector.mem,datacenter=us-east,host=local,metric_type=gauge,normal_tag=value,true_tag=true value=1000 1542182950000000011"
        );
    }

    fn counter(name: &str, value: f64, batch: &BatchNotifier) -> NormalizedMetric {
        NormalizedMetric {
            metric: Metric::new(
                name,
                MetricKind::Incremental,
                MetricValue::Counter { value },
            )
            .with_batch_notifier(batch),
            byte_size: 1,
            json_size: JsonSize::new(10),
        }
    }

    #[tokio::test]
    async fn batches_aggregate_series_and_count_every_event() {
        let (batch, _receiver) = BatchNotifier::new_with_receiver();
        let metrics = vec![
            counter("a", 1.0, &batch),
            counter("a", 2.0, &batch),
            counter("b", 1.0, &batch),
            counter("c", 1.0, &batch),
        ];
        let settings = BatcherSettings::new(
            Duration::from_secs(1),
            NonZeroUsize::MAX,
            NonZeroUsize::new(2).unwrap(),
        );
        let batches: Vec<MetricBatch> = stream::iter(metrics)
            .batched(MetricBatchConfig::new(settings))
            .collect()
            .await;

        let series = |batch: &MetricBatch| {
            batch
                .metrics
                .iter()
                .map(|metric| (metric.name().to_owned(), metric.value().clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(batches.len(), 2);
        assert_eq!(
            series(&batches[0]),
            vec![
                ("a".to_owned(), MetricValue::Counter { value: 3.0 }),
                ("b".to_owned(), MetricValue::Counter { value: 1.0 }),
            ]
        );
        assert_eq!(batches[0].count, 3);
        assert_eq!(batches[0].json_size, JsonSize::new(30));
        assert_eq!(
            series(&batches[1]),
            vec![("c".to_owned(), MetricValue::Counter { value: 1.0 })]
        );
        assert_eq!(batches[1].count, 1);
    }

    fn request_builder() -> InfluxDbMetricsRequestBuilder {
        InfluxDbMetricsRequestBuilder {
            write: InfluxDbWriteRequest::new(
                "http://localhost:9999/api/v2/write".parse().unwrap(),
                &SensitiveString::default(),
            ),
            protocol_version: ProtocolVersion::V2,
            default_namespace: None,
            tags: None,
            quantiles: Vec::new(),
        }
    }

    fn single_metric_request(metric: Metric) -> HttpRequest<Arc<MetricBatch>> {
        build_request(MetricBatch {
            metrics: vec![metric],
            count: 1,
            byte_size: 1,
            json_size: JsonSize::new(10),
        })
    }

    #[test]
    fn requests_release_finalizers_without_a_status() {
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let mut request = single_metric_request(counter("a", 1.0, &batch).metric);
        drop(batch);
        let retry = request.clone();

        // The driver gets no finalizers, so the response status does not reach the events.
        request
            .take_finalizers()
            .update_status(EventStatus::Rejected);
        request_builder().build(request).unwrap();
        assert!(receiver.try_recv().is_err());

        // The events are released once every copy of the request, such as the one kept for
        // retries, is dropped.
        drop(retry);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use chrono::{SecondsFormat, Utc};
    use futures::stream;
    use similar_asserts::assert_eq;
    use vector_lib::metric_tags;

    use crate::{
        config::{AcknowledgementsConfig, SinkConfig, SinkContext},
        event::{
            Event,
            metric::{Metric, MetricKind, MetricValue},
        },
        sinks::influxdb::{
            InfluxDb1Settings, InfluxDb2Settings, InfluxDbSettings,
            metrics::{InfluxDbConfig, default_summary_quantiles},
            test_util::{
                BUCKET, ORG, TOKEN, address_v1, address_v2, cleanup_v1, format_timestamp,
                onboarding_v1, onboarding_v2, query_v1,
            },
        },
        sinks::util::{BatchConfig, HttpEndpoint, TowerRequestConfig},
        test_util::components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        tls::{self, TlsConfig},
    };

    #[tokio::test]
    async fn inserts_metrics_v1_over_https() {
        insert_metrics_v1(
            address_v1(true).as_str(),
            Some(TlsConfig {
                ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
                ..Default::default()
            }),
        )
        .await;
    }

    #[tokio::test]
    async fn inserts_metrics_v1_over_http() {
        insert_metrics_v1(address_v1(false).as_str(), None).await;
    }

    async fn insert_metrics_v1(url: &str, tls: Option<TlsConfig>) {
        crate::test_util::trace_init();
        let database = onboarding_v1(url).await;

        let cx = SinkContext::default();

        let config = InfluxDbConfig {
            endpoint: HttpEndpoint::parse(url).unwrap(),
            settings: InfluxDbSettings::V1(InfluxDb1Settings {
                database: database.clone(),
                consistency: None,
                retention_policy_name: Some("autogen".to_string()),
                username: None,
                password: None,
            }),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls,
            quantiles: default_summary_quantiles(),
            tags: None,
            default_namespace: None,
            acknowledgements: AcknowledgementsConfig::default(),
        };

        let events: Vec<_> = (0..10).map(create_event).collect();
        let (sink, _) = config.build(cx).await.expect("error when building config");
        run_and_assert_sink_compliance(sink, stream::iter(events.clone()), &HTTP_SINK_TAGS).await;

        let res = query_v1_json(url, &format!("show series on {database}")).await;

        //
        // {"results":[{"statement_id":0,"series":[{"columns":["key"],"values":
        //  [
        //    ["ns.counter-0,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-1,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-2,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-3,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-4,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-5,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-6,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-7,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-8,metric_type=counter,production=true,region=us-west-1"],
        //    ["ns.counter-9,metric_type=counter,production=true,region=us-west-1"]
        //  ]}]}]}\n
        //

        assert_eq!(
            res["results"][0]["series"][0]["values"]
                .as_array()
                .unwrap()
                .len(),
            events.len()
        );

        for event in events {
            let metric = event.into_metric();
            let name = format!("{}.{}", metric.namespace().unwrap(), metric.name());
            let value = match metric.value() {
                MetricValue::Counter { value } => *value,
                _ => unreachable!(),
            };
            let timestamp = format_timestamp(metric.timestamp().unwrap(), SecondsFormat::Nanos);
            let res = query_v1_json(url, &format!("select * from {database}..\"{name}\"")).await;

            #[expect(clippy::cast_possible_truncation, reason = "Exact integer fixtures.")]
            let expected_value = value as isize;
            assert_eq!(
                res,
                serde_json::json! {
                    {"results": [{
                        "statement_id": 0,
                        "series": [{
                            "name": name,
                            "columns": ["time", "metric_type", "production", "region", "value"],
                            "values": [[timestamp, "counter", "true", "us-west-1", expected_value]]
                        }]
                    }]}
                }
            );
        }

        cleanup_v1(url, &database).await;
    }

    async fn query_v1_json(url: &str, query: &str) -> serde_json::Value {
        let string = query_v1(url, query)
            .await
            .text()
            .await
            .expect("Fetching text from InfluxDB query failed");
        serde_json::from_str(&string).expect("Error when parsing InfluxDB response JSON")
    }

    #[tokio::test]
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the complete encoding fixture together."
    )]
    async fn influxdb2_metrics_put_data() {
        crate::test_util::trace_init();
        let endpoint = address_v2();
        onboarding_v2(&endpoint).await;

        let cx = SinkContext::default();

        let config = InfluxDbConfig {
            endpoint: HttpEndpoint::parse(&endpoint).unwrap(),
            settings: InfluxDbSettings::V2(InfluxDb2Settings {
                org: ORG.to_string(),
                bucket: BUCKET.to_string(),
                token: TOKEN.to_string().into(),
            }),
            quantiles: default_summary_quantiles(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tags: None,
            tls: None,
            default_namespace: None,
            acknowledgements: AcknowledgementsConfig::default(),
        };

        let metric = format!(
            "counter-{}",
            Utc::now()
                .timestamp_nanos_opt()
                .expect("Timestamp out of range")
        );
        let mut events = Vec::new();
        for i in 0..10 {
            let event = Event::Metric(
                Metric::new(
                    metric.clone(),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: f64::from(i),
                    },
                )
                .with_namespace(Some("ns"))
                .with_tags(Some(metric_tags!(
                    "region" => "us-west-1",
                    "production" => "true",
                ))),
            );
            events.push(event);
        }

        let (sink, _) = config.build(cx).await.expect("error when building config");
        run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;

        let mut body = std::collections::HashMap::new();
        body.insert("query", format!("from(bucket:\"my-bucket\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"ns.{metric}\")"));
        body.insert("type", "flux".to_owned());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .post(format!("{}/api/v2/query?org=my-org", address_v2()))
            .json(&body)
            .header("accept", "application/json")
            .header("Authorization", "Token my-token")
            .send()
            .await
            .unwrap();
        let string = res.text().await.unwrap();

        let lines = string.split('\n').collect::<Vec<&str>>();
        let header = lines[0].split(',').collect::<Vec<&str>>();
        let record = lines[1].split(',').collect::<Vec<&str>>();

        assert_eq!(
            record[header
                .iter()
                .position(|&r| r.trim() == "metric_type")
                .unwrap()]
            .trim(),
            "counter"
        );
        assert_eq!(
            record[header
                .iter()
                .position(|&r| r.trim() == "production")
                .unwrap()]
            .trim(),
            "true"
        );
        assert_eq!(
            record[header.iter().position(|&r| r.trim() == "region").unwrap()].trim(),
            "us-west-1"
        );
        assert_eq!(
            record[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            format!("ns.{metric}")
        );
        assert_eq!(
            record[header.iter().position(|&r| r.trim() == "_field").unwrap()].trim(),
            "value"
        );
        assert_eq!(
            record[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "45"
        );
    }

    fn create_event(i: i32) -> Event {
        Event::Metric(
            Metric::new(
                format!("counter-{i}"),
                MetricKind::Incremental,
                MetricValue::Counter {
                    value: f64::from(i),
                },
            )
            .with_namespace(Some("ns"))
            .with_tags(Some(metric_tags!(
                "region" => "us-west-1",
                "production" => "true",
            )))
            .with_timestamp(Some(Utc::now())),
        )
    }
}
