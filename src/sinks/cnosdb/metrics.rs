use std::{collections::HashMap, future::ready, task::Poll};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::{future::BoxFuture, stream, SinkExt};
use tower::Service;
use vector_config::configurable_component;
use vector_core::{
    event::metric::{MetricSketch, Quantile},
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use crate::sinks::cnosdb::{build_line_protocol, healthcheck, CnosDBSettings, TYPE_TAG_KEY};
use crate::{
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::{
        metric::{Metric, MetricValue, Sample, StatisticKind},
        Event,
    },
    http::HttpClient,
    sinks::{
        util::{
            buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
            http::{HttpBatchService, HttpRetryLogic},
            statistic::{validate_quantiles, DistributionStatistic},
            BatchConfig, EncodedEvent, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsConfig, TlsSettings},
};

pub const DEFAULT_NAMESPACE: &str = "service";

pub const COUNTER_TYPE: &str = "counter";
pub const DISTRIBUTION_TYPE: &str = "distribution";
pub const GAUGE_TYPE: &str = "gauge";
pub const SET_TYPE: &str = "set";
pub const HISTOGRAM_TYPE: &str = "histogram";
pub const SUMMARY_TYPE: &str = "summary";
pub const SKETCH_TYPE: &str = "sketch";

pub const VALUE_KEY: &str = "value";

#[derive(Clone)]
struct CnosDBSvc {
    config: CnosDBConfig,
    inner: HttpBatchService<BoxFuture<'static, crate::Result<hyper::Request<Bytes>>>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CnosDBDefaultBatchSettings;

impl SinkBatchSettings for CnosDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `cnosdb_metrics` sink.
#[configurable_component(sink("cnosdb_metrics", "Deliver metric event data to CnosDB."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct CnosDBConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[serde(alias = "namespace")]
    #[configurable(metadata(docs::examples = "service"))]
    pub namespace: Option<String>,

    /// The endpoint to send data to.
    ///
    /// This should be a full HTTP URI, including the scheme, host, and port.
    #[configurable(metadata(docs::examples = "http://localhost:8902/"))]
    pub endpoint: String,

    #[serde(flatten)]
    pub settings: CnosDBSettings,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<CnosDBDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    /// A map of additional tags, in the key/value pair format, to add to each measurement.
    #[configurable(metadata(docs::additional_props_description = "A tag key/value pair."))]
    pub tags: Option<HashMap<String, String>>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The list of quantiles to calculate when sending distribution metrics.
    #[serde(default = "default_summary_quantiles")]
    pub quantiles: Vec<f64>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

pub fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}

impl_generate_config_from_default!(CnosDBConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "cnosdb_metrics")]
impl SinkConfig for CnosDBConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck =
            healthcheck(self.endpoint.clone(), self.settings.clone(), client.clone())?;
        validate_quantiles(&self.quantiles)?;
        let sink = CnosDBSvc::new(self.clone(), client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl CnosDBSvc {
    pub fn new(config: CnosDBConfig, client: HttpClient) -> crate::Result<VectorSink> {
        let endpoint = config.endpoint.clone();
        let auth = config.settings.authorization();

        let batch = config.batch.into_batch_settings()?;
        let request = config.request.unwrap_with(&TowerRequestConfig {
            retry_attempts: Some(5),
            ..Default::default()
        });

        let uri = config.settings.write_uri(endpoint)?;

        let http_service = HttpBatchService::new(client, create_build_request(uri, auth));

        let cnosdb_http_service = CnosDBSvc {
            config,
            inner: http_service,
        };
        let mut normalizer = MetricNormalizer::<CnosMetricNormalize>::default();

        let sink = request
            .batch_sink(
                HttpRetryLogic,
                cnosdb_http_service,
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
            .sink_map_err(|error| error!(message = "Fatal cnosdb sink error.", %error));

        #[allow(deprecated)]
        Ok(VectorSink::from_event_sink(sink))
    }
}

impl Service<Vec<Metric>> for CnosDBSvc {
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
            items,
            self.config.namespace.as_deref(),
            self.config.tags.as_ref(),
            &self.config.quantiles,
        );
        let body = input.freeze();

        self.inner.call(body)
    }
}

fn create_build_request(
    uri: http::Uri,
    auth: String,
) -> impl Fn(Bytes) -> BoxFuture<'static, crate::Result<hyper::Request<Bytes>>> + Sync + Send + 'static
{
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

fn merge_tags(event: &Metric, tags: Option<&HashMap<String, String>>) -> HashMap<String, String> {
    let mut line_tags = HashMap::new();
    if let Some(metric_tags) = event.tags() {
        line_tags = metric_tags
            .iter_all()
            .filter_map(|(k, v)| v.map(|v| (k.to_string(), v.to_string().replace(' ', "_"))))
            .collect::<HashMap<String, String>>();
    }
    if let Some(tags) = tags {
        line_tags.extend(
            tags.iter()
                .map(|(k, v)| (k.clone(), v.clone().replace(' ', "_"))),
        );
    }
    line_tags
}

#[derive(Default)]
pub struct CnosMetricNormalize;

impl MetricNormalize for CnosMetricNormalize {
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
    events: Vec<Metric>,
    default_namespace: Option<&str>,
    tags: Option<&HashMap<String, String>>,
    quantiles: &[f64],
) -> BytesMut {
    let mut output = String::new();

    for event in events.into_iter() {
        let table = format!(
            "{}.{}",
            event
                .namespace()
                .or(default_namespace)
                .unwrap_or(DEFAULT_NAMESPACE),
            event.name()
        );
        let ts = event
            .timestamp()
            .map(|ts| ts.timestamp_nanos())
            .unwrap_or_else(|| Utc::now().timestamp_nanos());
        let mut tags = merge_tags(&event, tags);
        let (metric_type, fields) = get_type_and_fields(event.value(), quantiles);
        tags.insert(TYPE_TAG_KEY.to_string(), metric_type.to_string());

        output.push_str(&build_line_protocol(&table, tags, fields, ts))
    }

    // remove last '\n'
    if !output.is_empty() {
        output.truncate(output.len() - 1);
    }
    BytesMut::from(output.as_str())
}

fn get_type_and_fields(
    value: &MetricValue,
    quantiles: &[f64],
) -> (&'static str, HashMap<String, String>) {
    match value {
        MetricValue::Counter { value } => (
            COUNTER_TYPE,
            HashMap::from([(VALUE_KEY.to_string(), value.to_string())]),
        ),
        MetricValue::Gauge { value } => (
            GAUGE_TYPE,
            HashMap::from([(VALUE_KEY.to_string(), value.to_string())]),
        ),
        MetricValue::Set { values } => (
            SET_TYPE,
            HashMap::from([(VALUE_KEY.to_string(), values.len().to_string() + "u")]),
        ),
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            let mut fields: HashMap<String, String> = buckets
                .iter()
                .map(|sample| {
                    (
                        format!("bucket_{}", sample.upper_limit),
                        sample.count.to_string() + "u",
                    )
                })
                .collect();
            fields.insert("count".to_owned(), count.to_string() + "u");
            fields.insert("sum".to_owned(), sum.to_string());

            (HISTOGRAM_TYPE, fields)
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            let mut fields: HashMap<String, String> = quantiles
                .iter()
                .map(|quantile| {
                    (
                        format!("quantile_{}", quantile.quantile),
                        quantile.value.to_string(),
                    )
                })
                .collect();
            fields.insert("count".to_owned(), count.to_string() + "u");
            fields.insert("sum".to_owned(), sum.to_string());

            (SUMMARY_TYPE, fields)
        }
        MetricValue::Distribution { samples, statistic } => {
            let quantiles = match statistic {
                StatisticKind::Histogram => &[0.95] as &[_],
                StatisticKind::Summary => quantiles,
            };
            let fields = encode_distribution(samples, quantiles);
            (DISTRIBUTION_TYPE, fields)
        }
        MetricValue::Sketch { sketch } => match sketch {
            MetricSketch::AgentDDSketch(ddsketch) => {
                // Hard-coded quantiles because cnosdb can't natively do anything useful with the
                // actual bins.
                let mut fields = [0.5, 0.75, 0.9, 0.99]
                    .iter()
                    .map(|q| {
                        let quantile = Quantile {
                            quantile: *q,
                            value: ddsketch.quantile(*q).unwrap_or(0.0),
                        };
                        (quantile.to_percentile_string(), quantile.value.to_string())
                    })
                    .collect::<HashMap<_, _>>();
                fields.insert(
                    "count".to_owned(),
                    u64::from(ddsketch.count()).to_string() + "u",
                );
                fields.insert(
                    "min".to_owned(),
                    ddsketch.min().unwrap_or(f64::MAX).to_string(),
                );
                fields.insert(
                    "max".to_owned(),
                    ddsketch.max().unwrap_or(f64::MIN).to_string(),
                );
                fields.insert("sum".to_owned(), ddsketch.sum().unwrap_or(0.0).to_string());
                fields.insert("avg".to_owned(), ddsketch.avg().unwrap_or(0.0).to_string());

                (SKETCH_TYPE, fields)
            }
        },
    }
}

fn encode_distribution(samples: &[Sample], quantiles: &[f64]) -> HashMap<String, String> {
    let statistic = match DistributionStatistic::from_samples(samples, quantiles) {
        None => return HashMap::new(),
        Some(statistic) => statistic,
    };

    let fields: HashMap<String, String> = vec![
        ("min".to_owned(), statistic.min.to_string()),
        ("max".to_owned(), statistic.max.to_string()),
        ("median".to_owned(), statistic.median.to_string()),
        ("avg".to_owned(), statistic.avg.to_string()),
        ("sum".to_owned(), statistic.sum.to_string()),
        ("count".to_owned(), statistic.count.to_string()),
    ]
    .into_iter()
    .chain(
        statistic
            .quantiles
            .iter()
            .map(|&(p, val)| (format!("quantile_{:.2}", p), val.to_string())),
    )
    .collect();

    fields
}
#[cfg(test)]
mod test {
    use super::get_type_and_fields;
    use crate::sinks::cnosdb::metrics::{
        encode_distribution, COUNTER_TYPE, GAUGE_TYPE, SET_TYPE, VALUE_KEY,
    };
    use std::collections::BTreeSet;
    use vector_core::event::metric::{Bucket, MetricSketch, Quantile, Sample};
    use vector_core::event::{MetricValue, StatisticKind};
    use vector_core::metrics::AgentDDSketch;

    #[test]
    fn test_cnosdb_sink_encode_distribution() {
        let samples = vec![
            Sample {
                value: 1.0,
                rate: 1,
            },
            Sample {
                value: 2.0,
                rate: 1,
            },
            Sample {
                value: 3.0,
                rate: 1,
            },
            Sample {
                value: 4.0,
                rate: 1,
            },
            Sample {
                value: 5.0,
                rate: 1,
            },
        ];
        let quantiles = &[0.5, 0.75, 0.9, 0.99];

        let fields = encode_distribution(&samples, quantiles);
        assert_eq!(fields.len(), 10);
        assert_eq!(fields["min"], "1");
        assert_eq!(fields["max"], "5");
        assert_eq!(fields["median"], "3");
        assert_eq!(fields["avg"], "3");
        assert_eq!(fields["sum"], "15");
        assert_eq!(fields["count"], "5");
        assert_eq!(fields["quantile_0.50"], "3");
        assert_eq!(fields["quantile_0.75"], "4");
        assert_eq!(fields["quantile_0.90"], "5");
        assert_eq!(fields["quantile_0.99"], "5");
    }

    #[test]
    fn test_cnosdb_sink_counter() {
        let value = MetricValue::Counter { value: 1.0 };
        let (metric_type, fields) = get_type_and_fields(&value, &[]);
        assert_eq!(metric_type, COUNTER_TYPE);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[VALUE_KEY], "1");
    }

    #[test]
    fn test_cnosdb_sink_gauge() {
        let value = MetricValue::Gauge { value: 1.0 };
        let (metric_type, fields) = get_type_and_fields(&value, &[]);
        assert_eq!(metric_type, GAUGE_TYPE);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[VALUE_KEY], "1");
    }

    #[test]
    fn test_cnosdb_sink_set() {
        let value = MetricValue::Set {
            values: BTreeSet::from(["1".to_string(), "2".to_string(), "3".to_string()]),
        };
        let (metric_type, fields) = get_type_and_fields(&value, &[]);
        assert_eq!(metric_type, SET_TYPE);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[VALUE_KEY], "3u");
    }

    #[test]
    fn test_cnosdb_sink_summary() {
        let value = MetricValue::AggregatedSummary {
            count: 5,
            sum: 15.0,
            quantiles: vec![
                Quantile {
                    quantile: 0.5,
                    value: 3.0,
                },
                Quantile {
                    quantile: 0.75,
                    value: 4.0,
                },
                Quantile {
                    quantile: 0.9,
                    value: 5.0,
                },
                Quantile {
                    quantile: 0.99,
                    value: 5.0,
                },
            ],
        };
        let (metric_type, fields) = get_type_and_fields(&value, &[]);
        assert_eq!(metric_type, "summary");
        assert_eq!(fields.len(), 6);
        assert_eq!(fields["sum"], "15");
        assert_eq!(fields["count"], "5u");
        assert_eq!(fields["quantile_0.5"], "3");
        assert_eq!(fields["quantile_0.75"], "4");
        assert_eq!(fields["quantile_0.9"], "5");
        assert_eq!(fields["quantile_0.99"], "5");
    }

    #[test]
    fn test_cnosdb_sink_histogram() {
        let value = MetricValue::AggregatedHistogram {
            count: 5,
            sum: 15.0,
            buckets: vec![
                Bucket {
                    upper_limit: 1.0,
                    count: 1,
                },
                Bucket {
                    upper_limit: 2.0,
                    count: 1,
                },
                Bucket {
                    upper_limit: 3.0,
                    count: 1,
                },
                Bucket {
                    upper_limit: 4.0,
                    count: 1,
                },
                Bucket {
                    upper_limit: 5.0,
                    count: 1,
                },
            ],
        };
        let (metric_type, fields) = get_type_and_fields(&value, &[]);
        assert_eq!(metric_type, "histogram");
        assert_eq!(fields.len(), 7);
        assert_eq!(fields["sum"], "15");
        assert_eq!(fields["count"], "5u");
        assert_eq!(fields["bucket_1"], "1u");
        assert_eq!(fields["bucket_2"], "1u");
        assert_eq!(fields["bucket_3"], "1u");
        assert_eq!(fields["bucket_4"], "1u");
        assert_eq!(fields["bucket_5"], "1u");
    }

    #[test]
    fn test_cnosdb_sink_distribution_summary() {
        let value = MetricValue::Distribution {
            samples: vec![
                Sample {
                    value: 1.0,
                    rate: 1,
                },
                Sample {
                    value: 2.0,
                    rate: 2,
                },
                Sample {
                    value: 3.0,
                    rate: 3,
                },
                Sample {
                    value: 4.0,
                    rate: 4,
                },
                Sample {
                    value: 5.0,
                    rate: 5,
                },
            ],
            statistic: StatisticKind::Summary,
        };
        let (metric_type, fields) = get_type_and_fields(&value, &[0.5, 0.75, 0.9, 0.99]);
        assert_eq!(metric_type, "distribution");
        assert_eq!(fields.len(), 10);
        assert_eq!(fields["min"], "1");
        assert_eq!(fields["max"], "5");
        assert_eq!(fields["median"], "4");
        assert_eq!(fields["avg"], "3.6666666666666665");
        assert_eq!(fields["sum"], "55");
        assert_eq!(fields["count"], "15");
        assert_eq!(fields["quantile_0.50"], "4");
        assert_eq!(fields["quantile_0.75"], "5");
        assert_eq!(fields["quantile_0.90"], "5");
        assert_eq!(fields["quantile_0.99"], "5");
    }

    #[test]
    fn test_cnosdb_sink_distribution_histogram() {
        let value = MetricValue::Distribution {
            samples: vec![
                Sample {
                    value: 1.0,
                    rate: 1,
                },
                Sample {
                    value: 2.0,
                    rate: 2,
                },
                Sample {
                    value: 3.0,
                    rate: 3,
                },
                Sample {
                    value: 4.0,
                    rate: 4,
                },
                Sample {
                    value: 5.0,
                    rate: 5,
                },
            ],
            statistic: StatisticKind::Histogram,
        };
        let (metric_type, fields) = get_type_and_fields(&value, &[0.5, 0.75, 0.9, 0.99]);
        assert_eq!(metric_type, "distribution");
        assert_eq!(fields.len(), 7);
        assert_eq!(fields["min"], "1");
        assert_eq!(fields["max"], "5");
        assert_eq!(fields["median"], "4");
        assert_eq!(fields["avg"], "3.6666666666666665");
        assert_eq!(fields["sum"], "55");
        assert_eq!(fields["count"], "15");
        assert_eq!(fields["quantile_0.95"], "5");
    }

    #[test]
    fn test_cnosdb_sink_sketch() {
        let value = MetricValue::Sketch {
            sketch: MetricSketch::AgentDDSketch(AgentDDSketch::with_agent_defaults()),
        };
        let (metric_type, fields) = get_type_and_fields(&value, &[0.5, 0.75, 0.9, 0.99]);
        assert_eq!(metric_type, "sketch");
        assert_eq!(fields.len(), 9);
        assert_eq!(fields["min"], f64::MAX.to_string());
        assert_eq!(fields["max"], f64::MIN.to_string());
        assert_eq!(fields["avg"], "0");
        assert_eq!(fields["sum"], "0");
        assert_eq!(fields["count"], "0u");
        assert_eq!(fields["50"], "0");
        assert_eq!(fields["99"], "0");
        assert_eq!(fields["90"], "0");
        assert_eq!(fields["75"], "0");
    }
}

#[cfg(feature = "cnosdb-integration-tests")]
#[cfg(test)]
mod test_integration {
    use crate::config::{SinkConfig, SinkContext};
    use crate::sinks::cnosdb::metrics::CnosDBConfig;
    use crate::test_util::components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS};
    use chrono::{TimeZone, Timelike, Utc};
    use futures_util::stream;
    use http::header::{ACCEPT, AUTHORIZATION};
    use vector_core::event::{Event, Metric, MetricKind, MetricValue};
    use vector_core::metric_tags;

    fn create_event(value: i32, i: u32) -> Event {
        Event::Metric(
            Metric::new(
                "counter".to_owned(),
                MetricKind::Incremental,
                MetricValue::Counter {
                    value: value as f64,
                },
            )
            .with_namespace(Some("counter"))
            .with_tags(Some(metric_tags!(
                "region" => "us-west-1",
                "production" => "true",
            )))
            .with_timestamp(Some(
                Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                    .single()
                    .and_then(|t| t.with_nanosecond(i))
                    .expect("invalid timestamp"),
            )),
        )
    }

    #[tokio::test]
    async fn test_cnosdb_sink() {
        let mut config = CnosDBConfig::default();
        config.endpoint =
            std::env::var("CNOSDB_ENDPOINT").unwrap_or("http://localhost:8902".to_string());
        let endpoint = if !config.endpoint.ends_with("/") {
            format!("{}/", config.endpoint)
        } else {
            config.endpoint.clone()
        };
        let (sink, _healthcheck) = config.build(SinkContext::default()).await.unwrap();
        let events = vec![create_event(1, 1), create_event(2, 2), create_event(3, 3)];
        run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
        let url = format!("{}api/v1/sql?tenant=cnosdb&db=public", endpoint);
        let client = reqwest::Client::new();
        let response = client
            .post(url)
            .header(AUTHORIZATION, "Basic cm9vdDo=")
            .header(ACCEPT, "application/table")
            .body(hyper::Body::from("SELECT * FROM \"counter.counter\""))
            .send()
            .await
            .unwrap();
        let res = response.text().await.unwrap();
        assert_eq!(
            res,
            "+-------------------------------+-------------+------------+-----------+-------+
             | time                          | metric_type | production | region    | value |
             +-------------------------------+-------------+------------+-----------+-------+
             | 2018-11-14T08:09:10.000000003 | counter     | true       | us-west-1 | 6.0   |
             +-------------------------------+-------------+------------+-----------+-------+"
        )
    }
}
