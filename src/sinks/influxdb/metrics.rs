#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use std::{collections::HashMap, future::ready, task::Poll};

use bytes::{Bytes, BytesMut};
use derivative::Derivative;
use futures::{SinkExt, future::BoxFuture, stream};
use indoc::indoc;
use tower::Service;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    configurable::configurable_component,
    event::metric::{MetricSketch, MetricTags, Quantile},
    sensitive_string::SensitiveString,
};

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::{
        Event, KeyString,
        metric::{Metric, MetricValue, Sample, StatisticKind},
    },
    http::HttpClient,
    internal_events::InfluxdbEncodingError,
    sinks::{
        Healthcheck, VectorSink,
        influxdb::{
            Field, InfluxDb1Settings, InfluxDb2Settings, InfluxDbSettings, InfluxDbVersion,
            ProtocolVersion, encode_timestamp, healthcheck, influx_line_protocol,
            influxdb_settings,
        },
        util::{
            BatchConfig, BatchSettings, EncodedEvent, HttpEndpoint, SinkBatchSettings,
            TowerRequestConfig,
            buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
            encode_namespace,
            http::{HttpBatchService, HttpRetryLogic},
            statistic::{DistributionStatistic, validate_quantiles},
        },
    },
    tls::{TlsConfig, TlsSettings},
};

#[derive(Clone)]
struct InfluxDbSvc {
    config: InfluxDbConfig,
    protocol_version: ProtocolVersion,
    inner: HttpBatchService<BoxFuture<'static, crate::Result<hyper::Request<Bytes>>>>,
}

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
#[serde(deny_unknown_fields)]
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

    /// The InfluxDB API version to use.
    ///
    /// Omitting this option is deprecated and it will be required in a future release. When
    /// unset, the version is temporarily inferred from the configured settings.
    #[configurable(metadata(docs::examples = "2"))]
    #[configurable(metadata(docs::examples = "1"))]
    #[configurable(metadata(docs::minimal = true))]
    pub version: Option<InfluxDbVersion>,

    /// The name of the database to write into.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "vector-database"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    #[configurable(metadata(docs::required_when = "version = \"1\""))]
    pub database: Option<String>,

    /// The consistency level to use for writes.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "any"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub consistency: Option<String>,

    /// The target retention policy for writes.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "autogen"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub retention_policy_name: Option<String>,

    /// The username to authenticate with.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "todd"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub username: Option<String>,

    /// The password to authenticate with.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "${INFLUXDB_PASSWORD}"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub password: Option<SensitiveString>,

    /// The name of the organization to write into.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    #[configurable(metadata(docs::examples = "my-org"))]
    #[configurable(metadata(docs::relevant_when = "version = \"2\""))]
    #[configurable(metadata(docs::required_when = "version = \"2\""))]
    #[configurable(metadata(docs::minimal = true))]
    pub org: Option<String>,

    /// The name of the bucket to write into.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    #[configurable(metadata(docs::examples = "vector-bucket"))]
    #[configurable(metadata(docs::relevant_when = "version = \"2\""))]
    #[configurable(metadata(docs::required_when = "version = \"2\""))]
    #[configurable(metadata(docs::minimal = true))]
    pub bucket: Option<String>,

    /// The [token][token_docs] to authenticate with.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    ///
    /// [token_docs]: https://v2.docs.influxdata.com/v2.0/security/tokens/
    #[configurable(metadata(docs::examples = "${INFLUXDB_TOKEN}"))]
    #[configurable(metadata(docs::relevant_when = "version = \"2\""))]
    #[configurable(metadata(docs::required_when = "version = \"2\""))]
    #[configurable(metadata(docs::minimal = true))]
    pub token: Option<SensitiveString>,

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

pub fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}

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

impl InfluxDbConfig {
    fn settings(&self) -> crate::Result<InfluxDbSettings> {
        let version = match self.version {
            Some(version) => {
                self.validate_version(version)?;
                version
            }
            None => {
                warn!(
                    "The `version` option is currently optional but will be required in a future release. \
                     Please set it to `1` or `2` to match your InfluxDB settings."
                );
                self.infer_version()?
            }
        };
        match version {
            InfluxDbVersion::V1 => Ok(InfluxDbSettings::V1(InfluxDb1Settings {
                database: self
                    .database
                    .clone()
                    .ok_or("the `database` option is required when using InfluxDB v1")?,
                consistency: self.consistency.clone(),
                retention_policy_name: self.retention_policy_name.clone(),
                username: self.username.clone(),
                password: self.password.clone(),
            })),
            InfluxDbVersion::V2 => Ok(InfluxDbSettings::V2(InfluxDb2Settings {
                org: self
                    .org
                    .clone()
                    .ok_or("the `org` option is required when using InfluxDB v2")?,
                bucket: self
                    .bucket
                    .clone()
                    .ok_or("the `bucket` option is required when using InfluxDB v2")?,
                token: self
                    .token
                    .clone()
                    .ok_or("the `token` option is required when using InfluxDB v2")?,
            })),
        }
    }

    const fn settings_present(&self) -> (bool, bool) {
        let has_v1 = self.database.is_some()
            || self.consistency.is_some()
            || self.retention_policy_name.is_some()
            || self.username.is_some()
            || self.password.is_some();
        let has_v2 = self.org.is_some() || self.bucket.is_some() || self.token.is_some();
        (has_v1, has_v2)
    }

    fn infer_version(&self) -> crate::Result<InfluxDbVersion> {
        let (has_v1, has_v2) = self.settings_present();
        match (has_v1, has_v2) {
            (true, true) => Err(
                "Unclear settings. Both InfluxDB v1 and v2 settings are configured; configure only one version."
                    .into(),
            ),
            (false, false) => Err("InfluxDB v1 or v2 should be configured as endpoint.".into()),
            (true, false) => Ok(InfluxDbVersion::V1),
            (false, true) => Ok(InfluxDbVersion::V2),
        }
    }

    /// Rejects settings that belong to the version that was not selected, so that
    /// an explicit `version` cannot silently ignore stale settings for the other version.
    fn validate_version(&self, version: InfluxDbVersion) -> crate::Result<()> {
        let (has_v1, has_v2) = self.settings_present();
        match version {
            InfluxDbVersion::V1 if has_v2 => Err(
                "InfluxDB v1 settings are configured, but v2 settings were also provided; configure only one version."
                    .into(),
            ),
            InfluxDbVersion::V2 if has_v1 => Err(
                "InfluxDB v2 settings are configured, but v1 settings were also provided; configure only one version."
                    .into(),
            ),
            _ => Ok(()),
        }
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

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ValidatedInfluxDbMetrics {
    // Omitted: the retained `uri` embeds the v1 password in its `p` query parameter.
    #[derivative(Debug = "ignore")]
    uri: http::Uri,
    token: SensitiveString,
    protocol_version: ProtocolVersion,
    #[derivative(Debug = "ignore")]
    batch: BatchSettings<MetricsBuffer>,
}

#[async_trait::async_trait]
impl ValidatedSink for InfluxDbConfig {
    type Validated = ValidatedInfluxDbMetrics;

    fn validate(&self) -> crate::Result<ValidatedInfluxDbMetrics> {
        validate_quantiles(&self.quantiles)?;

        let settings = influxdb_settings(self.settings()?);

        let uri = settings.write_uri(self.endpoint.clone())?;

        let token = settings.token();
        let protocol_version = settings.protocol_version();

        let batch = self.batch.into_batch_settings()?;

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
        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck = healthcheck(self.clone().endpoint, self.settings()?, client.clone())?;
        let sink = InfluxDbSvc::from_validated(self.clone(), validated, client)?;
        Ok((sink, healthcheck))
    }
}

impl InfluxDbSvc {
    #[cfg(all(test, feature = "influxdb-integration-tests"))]
    pub fn new(config: InfluxDbConfig, client: HttpClient) -> crate::Result<VectorSink> {
        let validated = config.validate()?;
        Self::from_validated(config, &validated, client)
    }

    fn from_validated(
        config: InfluxDbConfig,
        validated: &ValidatedInfluxDbMetrics,
        client: HttpClient,
    ) -> crate::Result<VectorSink> {
        let ValidatedInfluxDbMetrics {
            uri,
            token,
            protocol_version,
            batch,
        } = validated;

        let request = config.request.into_settings();

        let http_service =
            HttpBatchService::new(client, create_build_request(uri.clone(), token.inner()));

        let influxdb_http_service = InfluxDbSvc {
            config,
            protocol_version: *protocol_version,
            inner: http_service,
        };
        let mut normalizer = MetricNormalizer::<InfluxMetricNormalize>::default();

        let sink = request
            .batch_sink(
                HttpRetryLogic::default(),
                influxdb_http_service,
                MetricsBuffer::new(batch.size),
                batch.timeout,
            )
            .with_flat_map(move |event: Event| {
                stream::iter({
                    let byte_size = event.size_of();
                    let json_size = event.estimated_json_encoded_size_of();

                    normalizer
                        .normalize(event.into_metric())
                        .map(|metric| Ok(EncodedEvent::new(metric, byte_size, json_size)))
                })
            })
            .sink_map_err(|error| error!(message = "Fatal influxdb sink error.", %error, internal_log_rate_limit = false));

        #[allow(deprecated)]
        Ok(VectorSink::from_event_sink(sink))
    }
}

impl Service<Vec<Metric>> for InfluxDbSvc {
    type Response = http::Response<Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller
    fn poll_ready(&mut self, cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let input = encode_events(
            self.protocol_version,
            items,
            self.config.default_namespace.as_deref(),
            self.config.tags.as_ref(),
            &self.config.quantiles,
        );
        let body = input.freeze();

        self.inner.call(body)
    }
}

fn create_build_request(
    uri: http::Uri,
    token: &str,
) -> impl Fn(Bytes) -> BoxFuture<'static, crate::Result<hyper::Request<Bytes>>>
+ Sync
+ Send
+ 'static
+ use<> {
    let auth = format!("Token {token}");
    move |body| {
        Box::pin(ready(
            hyper::Request::post(uri.clone())
                .header("Content-Type", "text/plain")
                .header("Authorization", auth.clone())
                .body(body)
                .map_err(Into::into),
        ))
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
    events: Vec<Metric>,
    default_namespace: Option<&str>,
    tags: Option<&HashMap<String, String>>,
    quantiles: &[f64],
) -> BytesMut {
    let mut output = BytesMut::new();
    let count = events.len();

    for event in events.into_iter() {
        let fullname = encode_namespace(event.namespace().or(default_namespace), '.', event.name());
        let ts = encode_timestamp(event.timestamp());
        let tags = merge_tags(&event, tags);
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
        };
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
        MetricValue::Set { values } => ("set", Some(to_fields(values.len() as f64))),
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

    Some(
        [
            ("min".into(), Field::Float(statistic.min)),
            ("max".into(), Field::Float(statistic.max)),
            ("median".into(), Field::Float(statistic.median)),
            ("avg".into(), Field::Float(statistic.avg)),
            ("sum".into(), Field::Float(statistic.sum)),
            ("count".into(), Field::Float(statistic.count as f64)),
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
    use indoc::indoc;
    use similar_asserts::assert_eq;

    use super::*;
    use crate::{
        config::ValidatedSink,
        event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
        sinks::influxdb::test_util::{assert_fields, split_line_protocol, tags, ts},
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
    fn prepares_valid_config() {
        let config = InfluxDbConfig {
            endpoint: HttpEndpoint::parse("http://localhost:9999").unwrap(),
            org: Some("my-org".to_string()),
            bucket: Some("my-bucket".to_string()),
            token: Some("my-token".to_string().into()),
            default_namespace: None,
            version: Some(InfluxDbVersion::V2),
            database: None,
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
            batch: Default::default(),
            request: Default::default(),
            tags: None,
            tls: None,
            quantiles: default_summary_quantiles(),
            acknowledgements: Default::default(),
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, Some("vector"), None, &[]);
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
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

        let line_protocols = encode_events(ProtocolVersion::V1, events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol1.0);
        assert_eq!(
            "metric_type=histogram,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "bucket_1=1i",
                "bucket_2.1=2i",
                "bucket_3=3i",
                "count=6i",
                "sum=12.5",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol1.0);
        assert_eq!(
            "metric_type=histogram,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "bucket_1=1u",
                "bucket_2.1=2u",
                "bucket_3=3u",
                "count=6u",
                "sum=12.5",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
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

        let line_protocols = encode_events(ProtocolVersion::V1, events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests_sum", line_protocol1.0);
        assert_eq!(
            "metric_type=summary,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "count=6i",
                "quantile_0.01=1.5",
                "quantile_0.5=2",
                "quantile_0.99=3",
                "sum=12",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 1);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests_sum", line_protocol1.0);
        assert_eq!(
            "metric_type=summary,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
            [
                "count=6u",
                "quantile_0.01=1.5",
                "quantile_0.5=2",
                "quantile_0.99=3",
                "sum=12",
            ]
            .to_vec(),
        );
        assert_eq!("1542182950000000011", line_protocol1.3);
    }

    #[test]
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
        let line_protocols =
            String::from_utf8(line_protocols.freeze().as_ref().to_owned()).unwrap();
        let line_protocols: Vec<&str> = line_protocols.split('\n').collect();
        assert_eq!(line_protocols.len(), 3);

        let line_protocol1 = split_line_protocol(line_protocols[0]);
        assert_eq!("ns.requests", line_protocol1.0);
        assert_eq!(
            "metric_type=distribution,normal_tag=value,true_tag=true",
            line_protocol1.1
        );
        assert_fields(
            line_protocol1.2.to_string(),
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
        assert_eq!("1542182950000000011", line_protocol1.3);

        let line_protocol2 = split_line_protocol(line_protocols[1]);
        assert_eq!("ns.dense_stats", line_protocol2.0);
        assert_eq!("metric_type=distribution", line_protocol2.1);
        assert_fields(
            line_protocol2.2.to_string(),
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
        assert_eq!("1542182950000000011", line_protocol2.3);

        let line_protocol3 = split_line_protocol(line_protocols[2]);
        assert_eq!("ns.sparse_stats", line_protocol3.0);
        assert_eq!("metric_type=distribution", line_protocol3.1);
        assert_fields(
            line_protocol3.2.to_string(),
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
        assert_eq!("1542182950000000011", line_protocol3.3);
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
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

        let line_protocols = encode_events(ProtocolVersion::V2, events, None, None, &[]);
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
            events,
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
            line_protocol.2.to_string(),
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
            events,
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
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use chrono::{SecondsFormat, Utc};
    use futures::stream;
    use similar_asserts::assert_eq;
    use vector_lib::metric_tags;

    use crate::{
        config::{SinkConfig, SinkContext},
        event::{
            Event,
            metric::{Metric, MetricKind, MetricValue},
        },
        http::HttpClient,
        sinks::influxdb::{
            InfluxDbVersion,
            metrics::{InfluxDbConfig, InfluxDbSvc, default_summary_quantiles},
            test_util::{
                BUCKET, ORG, TOKEN, address_v1, address_v2, cleanup_v1, format_timestamp,
                onboarding_v1, onboarding_v2, query_v1,
            },
        },
        sinks::util::HttpEndpoint,
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
        .await
    }

    #[tokio::test]
    async fn inserts_metrics_v1_over_http() {
        insert_metrics_v1(address_v1(false).as_str(), None).await
    }

    async fn insert_metrics_v1(url: &str, tls: Option<TlsConfig>) {
        crate::test_util::trace_init();
        let database = onboarding_v1(url).await;

        let cx = SinkContext::default();

        let config = InfluxDbConfig {
            endpoint: HttpEndpoint::parse(url).unwrap(),
            version: Some(InfluxDbVersion::V1),
            database: Some(database.clone()),
            consistency: None,
            retention_policy_name: Some("autogen".to_string()),
            username: None,
            password: None,
            org: None,
            bucket: None,
            token: None,
            batch: Default::default(),
            request: Default::default(),
            tls,
            quantiles: default_summary_quantiles(),
            tags: None,
            default_namespace: None,
            acknowledgements: Default::default(),
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

            assert_eq!(
                res,
                serde_json::json! {
                    {"results": [{
                        "statement_id": 0,
                        "series": [{
                            "name": name,
                            "columns": ["time", "metric_type", "production", "region", "value"],
                            "values": [[timestamp, "counter", "true", "us-west-1", value as isize]]
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
    async fn influxdb2_metrics_put_data() {
        crate::test_util::trace_init();
        let endpoint = address_v2();
        onboarding_v2(&endpoint).await;

        let cx = SinkContext::default();

        let config = InfluxDbConfig {
            endpoint: HttpEndpoint::parse(&endpoint).unwrap(),
            version: Some(InfluxDbVersion::V2),
            database: None,
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
            org: Some(ORG.to_string()),
            bucket: Some(BUCKET.to_string()),
            token: Some(TOKEN.to_string().into()),
            quantiles: default_summary_quantiles(),
            batch: Default::default(),
            request: Default::default(),
            tags: None,
            tls: None,
            default_namespace: None,
            acknowledgements: Default::default(),
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
                    MetricValue::Counter { value: i as f64 },
                )
                .with_namespace(Some("ns"))
                .with_tags(Some(metric_tags!(
                    "region" => "us-west-1",
                    "production" => "true",
                ))),
            );
            events.push(event);
        }

        let client = HttpClient::new(None, cx.proxy()).unwrap();
        let sink = InfluxDbSvc::new(config, client).unwrap();
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
            format!("ns.{}", metric)
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
                MetricValue::Counter { value: i as f64 },
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
