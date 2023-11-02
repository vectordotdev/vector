use std::{collections::HashMap};

use vector_lib::configurable::configurable_component;
use vector_lib::configurable::component::GenerateConfig;
use vector_lib::{config::LogNamespace, event::{metric::Sample, StatisticKind, metric::{Bucket, Quantile}}};
use vector_lib::event::LogEvent;
use chrono::Utc;
use vrl::event_path;

use crate::{
    config::{
        DataType, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::{
        metric::{Metric, MetricKind, MetricValue, MetricTags},
        Event, Value,
    },
    internal_events::{
        MetricMetadataMetricDetailsNotFoundError, MetricMetadataInvalidFieldValueError, MetricMetadataParseFloatError,
        MetricMetadataParseArrayError, MetricMetadataParseIntError, ParserMissingFieldError, DROP_EVENT,
    },
    schema,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `metric_metadata` transform.
#[configurable_component(transform("metric_metadata", "Convert log events to metric events."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct MetricsMetadataConfig {}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MetricsMetadata {
    config: MetricsMetadataConfig,
}

impl GenerateConfig for MetricsMetadataConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "metric_metadata")]
impl TransformConfig for MetricsMetadataConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(MetricsMetadata::new(self.clone())))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        _: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(DataType::Metric, HashMap::new())]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

impl MetricsMetadata {
    pub const fn new(config: MetricsMetadataConfig) -> Self {
        MetricsMetadata { config }
    }
}

enum TransformError {
    MetricValueError {
        field: String,
        field_value: String,
    },
    FieldNotFound {
        field: String
    },
    MetricDetailsNotFound,
    ParseFloatError {
        field: String
    },
    ParseIntError {
        field: String
    },
    ParseArrayError {
        field: String
    },
}

fn bytes_to_str(value: &Value) -> Option<String> {
    match value {
        Value::Bytes(bytes) => std::str::from_utf8(bytes).ok().map(|s| s.to_string()),
        _ => None,
    }
}

fn get_str_from_log(log: &LogEvent, key: &str) -> Option<String> {
    log.get(event_path!(key)).and_then(bytes_to_str)
}

fn get_counter_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let counter_value = log.get("counter.value")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "counter.value".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseFloatError {
            field: "counter.value".to_string()
        })?;

    Ok(MetricValue::Counter { value: *counter_value })
}

fn get_gauge_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let gauge_value = log.get("gauge.value")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "gauge.value".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseFloatError {
            field: "gauge.value".to_string()
        })?;
    Ok(MetricValue::Gauge { value: *gauge_value })
}

fn get_distribution_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let event_samples = log.get("distribution.samples")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "distribution.samples".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseArrayError {
            field: "distribution.samples".to_string()
        })?;

    let mut samples: Vec<Sample> = Vec::new();
    for e_sample in event_samples {
        let value = e_sample.get("value")
            .ok_or_else(|| TransformError::FieldNotFound {
                field: "value".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseFloatError {
                field: "value".to_string()
            })?;

        let rate = e_sample.get("rate")
            .ok_or_else(|| TransformError::FieldNotFound {
                field: "rate".to_string(),
            })?
            .as_integer()
            .ok_or_else(|| TransformError::ParseIntError {
                field: "rate".to_string()
            })?;

        samples.push(Sample { value: *value, rate: rate as u32 });
    }

    let statistic_str = get_str_from_log(&log, "distribution.statistic")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "distribution.statistic".to_string(),
        })?;

    let statistic_kind = match statistic_str.as_str() {
        "histogram" => Ok(StatisticKind::Histogram),
        "summary" => Ok(StatisticKind::Summary),
        _ => Err(TransformError::MetricValueError {
            field: "distribution.statistic".to_string(),
            field_value: statistic_str.to_string(),
        }),
    }?;

    Ok(MetricValue::Distribution {
        samples,
        statistic: statistic_kind,
    })
}

fn get_histogram_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let event_buckets = log.get("histogram.buckets")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "histogram.buckets".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseArrayError {
            field: "histogram.buckets".to_string()
        })?;

    let mut buckets: Vec<Bucket> = Vec::new();
    for e_bucket in event_buckets {
        let upper_limit = e_bucket.get("upper_limit")
            .ok_or_else(|| TransformError::FieldNotFound {
                field: "histogram.buckets.upper_limit".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseFloatError {
                field: "histogram.buckets.upper_limit".to_string()
            })?;

        let count = e_bucket.get("count")
            .ok_or_else(|| TransformError::FieldNotFound {
                field: "histogram.buckets.count".to_string(),
            })?
            .as_integer()
            .ok_or_else(|| TransformError::ParseIntError {
                field: "histogram.buckets.count".to_string()
            })?;

        buckets.push(
            Bucket {
                upper_limit: *upper_limit,
                count: count as u64,
            }
        );
    }

    let count = log.get("histogram.count")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "histogram.count".to_string(),
        })?
        .as_integer()
        .ok_or_else(|| TransformError::ParseIntError {
            field: "histogram.count".to_string()
        })?;

    let sum = log.get("histogram.sum")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "histogram.sum".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseFloatError {
            field: "histogram.sum".to_string()
        })?;

    Ok(MetricValue::AggregatedHistogram {
        buckets,
        count: count as u64,
        sum: *sum,
    })
}

fn get_summary_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let event_quantiles = log.get("summary.quantiles")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "summary.quantiles".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseArrayError {
            field: "summary.quantiles".to_string()
        })?;

    let mut quantiles: Vec<Quantile> = Vec::new();
    for e_quantile in event_quantiles {
        let quantile = e_quantile.get("quantile")
            .ok_or_else(|| TransformError::FieldNotFound {
                field: "summary.quantiles.quantile".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseFloatError {
                field: "summary.quantiles.quantile".to_string()
            })?;

        let value = e_quantile.get("value")
            .ok_or_else(|| TransformError::FieldNotFound {
                field: "summary.quantiles.value".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseFloatError {
                field: "summary.quantiles.value".to_string()
            })?;

        quantiles.push(
            Quantile {
                quantile: *quantile,
                value: *value,
            }
        )
    }

    let count = log.get("summary.count")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "summary.count".to_string(),
        })?
        .as_integer()
        .ok_or_else(|| TransformError::ParseIntError {
            field: "summary.count".to_string()
        })?;

    let sum = log.get("summary.sum")
        .ok_or_else(|| TransformError::FieldNotFound {
            field: "summary.sum".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseFloatError {
            field: "summary.sum".to_string()
        })?;

    Ok(MetricValue::AggregatedSummary {
        quantiles,
        count: count as u64,
        sum: *sum,
    })
}

fn to_metric(event: &Event) -> Result<Metric, TransformError> {
    let log = event.as_log();
    let timestamp = log
        .get_timestamp()
        .and_then(Value::as_timestamp)
        .cloned()
        .or_else(|| Some(Utc::now()));

    let name = match get_str_from_log(&log, "name") {
        Some(n) => n,
        None => return Err(TransformError::FieldNotFound {
            field: "name".to_string()
        }),
    };

    let tags = &mut MetricTags::default();

    if let Some(els) = log.get("tags") {
        if let Some(el) = els.as_object() {
            for (key, value) in el {
                tags.insert(String::from(key).to_string(), bytes_to_str(value));
            }
        }
    }
    let tags_result = Some(tags.clone());

    let kind_str = get_str_from_log(&log, "kind").ok_or_else(|| TransformError::FieldNotFound {
        field: "kind".to_string(),
    })?;
    let kind = match kind_str.as_str() {
        "absolute" => Ok(MetricKind::Absolute),
        "incremental" => Ok(MetricKind::Incremental),
        value => Err(TransformError::MetricValueError { field: "kind".to_string(), field_value: value.to_string() })
    }?;

    let mut value: Option<MetricValue> = None;
    if let Some(root_event) = log.as_map() {
        for (key, _v) in root_event {
            value = match key.as_str() {
                "gauge" => Some(get_gauge_value(&log)?),
                "distribution" => Some(get_distribution_value(&log)?),
                "histogram" => Some(get_histogram_value(&log)?),
                "summary" => Some(get_summary_value(&log)?),
                "counter" => Some(get_counter_value(&log)?),
                _ => { None }
            };

            if value.is_some() {
                break;
            }
        }
    }

    let value = value.ok_or(TransformError::MetricDetailsNotFound)?;

    Ok(Metric::new_with_metadata(name, kind, value, log.metadata().clone())
        .with_namespace(get_str_from_log(log, "namespace"))
        .with_tags(tags_result)
        .with_timestamp(timestamp))
}

impl FunctionTransform for MetricsMetadata {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        match to_metric(&event) {
            Ok(metric) => {
                output.push(Event::Metric(metric));
            }
            Err(err) => {
                match err {
                    TransformError::MetricValueError { field, field_value } => emit!(MetricMetadataInvalidFieldValueError {
                            field: field.as_ref(),
                            field_value: field_value.as_ref()
                        }),
                    TransformError::FieldNotFound { field } => {
                        emit!(ParserMissingFieldError::<DROP_EVENT> {
                                field: field.as_ref()
                        })
                    }
                    TransformError::ParseFloatError { field } => {
                        emit!(MetricMetadataParseFloatError {
                                field: field.as_ref()
                            })
                    }
                    TransformError::ParseIntError { field } => {
                        emit!(MetricMetadataParseIntError {
                                field: field.as_ref()
                            })
                    }
                    TransformError::ParseArrayError { field } => {
                        emit!(MetricMetadataParseArrayError {
                                field: field.as_ref()
                            })
                    }
                    TransformError::MetricDetailsNotFound {} => {
                        emit!(MetricMetadataMetricDetailsNotFoundError {
                            })
                    }
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use std::sync::Arc;

    use super::*;
    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use crate::test_util::components::assert_transform_compliance;
    use crate::transforms::test::create_topology;
    use vector_lib::event::{EventMetadata};
    use vector_lib::event::LogEvent;
    use vector_lib::metric_tags;
    use crate::config::ComponentKey;

    fn create_log_event(json_str : &str) -> LogEvent {
        let mut log_value: Value = serde_json::from_str(&*json_str).expect("JSON was not well-formatted");
        log_value.insert("timestamp", ts());
        log_value.insert("namespace", "test_namespace");

        let mut metadata = EventMetadata::default();
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));

        LogEvent::from_parts(log_value, metadata.clone())
    }

    async fn do_transform(log: LogEvent) -> Option<Metric> {
        assert_transform_compliance(async move {
            let config = MetricsMetadataConfig {
                ..Default::default()
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            tx.send(log.into()).await.unwrap();

            let result = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);

            result
        })
            .await
            .map(|e| e.into_metric())
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }


    #[tokio::test]
    async fn transform_gauge() {
        let json_str = r#"{
          "gauge": {
            "value": 990.0
          },
          "kind": "absolute",
          "name": "test.transform.gauge",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);

        let metric = do_transform(log.clone()).await.unwrap();
        assert_eq!(
            metric,
            Metric::new_with_metadata(
                "test.transform.gauge",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 990.0 },
                metric.metadata().clone(),
            )
                .with_namespace(Some("test_namespace"))
                .with_tags(Some(metric_tags!(
                "env" => "test_env",
                "host" => "localhost",
            )))
                .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn transform_histogram() {
        let json_str = r#"{
          "histogram": {
            "sum": 18.0,
            "count": 5,
            "buckets": [
              {
                "upper_limit": 1.0,
                "count": 1
              },
              {
                "upper_limit": 2.0,
                "count": 2
              },
              {
                "upper_limit": 5.0,
                "count": 1
              },
              {
                "upper_limit": 10.0,
                "count": 1
              }
            ]
          },
          "kind": "absolute",
          "name": "test.transform.histogram",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);

        let metric = do_transform(log.clone()).await.unwrap();
        assert_eq!(
            metric,
            Metric::new_with_metadata(
                "test.transform.histogram",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    count: 5,
                    sum: 18.0,
                    buckets: vec![
                        Bucket {
                            upper_limit: 1.0,
                            count: 1,
                        },
                        Bucket {
                            upper_limit: 2.0,
                            count: 2,
                        },
                        Bucket {
                            upper_limit: 5.0,
                            count: 1,
                        },
                        Bucket {
                            upper_limit: 10.0,
                            count: 1,
                        },
                    ],
                },
                metric.metadata().clone(),
            )
                .with_namespace(Some("test_namespace"))
                .with_tags(Some(metric_tags!(
                "env" => "test_env",
                "host" => "localhost",
            )))
                .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn transform_distribution_histogram() {
        let json_str = r#"{
          "distribution": {
            "samples": [
              {
                "value": 1.0,
                "rate": 1
              },
              {
                "value": 2.0,
                "rate": 2
              }
            ],
            "statistic": "histogram"
          },
          "kind": "absolute",
          "name": "test.transform.distribution_histogram",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);

        let metric = do_transform(log.clone()).await.unwrap();

        assert_eq!(
            metric,
            Metric::new_with_metadata(
                "test.transform.distribution_histogram",
                MetricKind::Absolute,
                MetricValue::Distribution {
                    samples: vec![
                        Sample { value: 1.0, rate: 1 },
                        Sample { value: 2.0, rate: 2 },
                    ],
                    statistic: StatisticKind::Histogram,
                },
                metric.metadata().clone(),
            )
                .with_namespace(Some("test_namespace"))
                .with_tags(Some(metric_tags!(
                "env" => "test_env",
                "host" => "localhost",
            )))
                .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn transform_distribution_summary() {
        let json_str = r#"{
          "distribution": {
            "samples": [
              {
                "value": 1.0,
                "rate": 1
              },
              {
                "value": 2.0,
                "rate": 2
              }
            ],
            "statistic": "summary"
          },
          "kind": "absolute",
          "name": "test.transform.distribution_summary",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);

        let metric = do_transform(log.clone()).await.unwrap();
        assert_eq!(
            metric,
            Metric::new_with_metadata(
                "test.transform.distribution_summary",
                MetricKind::Absolute,
                MetricValue::Distribution {
                    samples: vec![
                        Sample { value: 1.0, rate: 1 },
                        Sample { value: 2.0, rate: 2 },
                    ],
                    statistic: StatisticKind::Summary,
                },
                metric.metadata().clone(),
            )
                .with_namespace(Some("test_namespace"))
                .with_tags(Some(metric_tags!(
                "env" => "test_env",
                "host" => "localhost",
            )))
                .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn transform_summary() {
        let json_str = r#"{
          "summary": {
            "sum": 100.0,
            "count": 7,
            "quantiles": [
              {
                "quantile": 0.05,
                "value": 10.0
              },
              {
                "quantile": 0.95,
                "value": 25.0
              }
            ]
          },
          "kind": "absolute",
          "name": "test.transform.histogram",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);

        let metric = do_transform(log.clone()).await.unwrap();
        assert_eq!(
            metric,
            Metric::new_with_metadata(
                "test.transform.histogram",
                MetricKind::Absolute,
                MetricValue::AggregatedSummary {
                    quantiles: vec![
                        Quantile {
                            quantile: 0.05,
                            value: 10.0,
                        },
                        Quantile {
                            quantile: 0.95,
                            value: 25.0,
                        },
                    ],
                    count: 7,
                    sum: 100.0,
                },
                metric.metadata().clone(),
            )
                .with_namespace(Some("test_namespace"))
                .with_tags(Some(metric_tags!(
                "env" => "test_env",
                "host" => "localhost",
            )))
                .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn transform_counter() {
        let json_str = r#"{
          "counter": {
            "value": 10.0
          },
          "kind": "incremental",
          "name": "test.transform.counter",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);

        let metric = do_transform(log.clone()).await.unwrap();
        assert_eq!(
            metric,
            Metric::new_with_metadata(
                "test.transform.counter",
                MetricKind::Incremental,
                MetricValue::Counter { value: 10.0 },
                metric.metadata().clone(),
            )
                .with_namespace(Some("test_namespace"))
                .with_tags(Some(metric_tags!(
                "env" => "test_env",
                "host" => "localhost",
            )))
                .with_timestamp(Some(ts()))
        );
    }
}
