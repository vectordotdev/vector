use crate::{
    config::{log_schema, DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::{self, Event, LogEvent},
    internal_events::{MetricToLogEventProcessed, MetricToLogFailedSerialize},
    transforms::{FunctionTransform, Transform},
    types::Conversion,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MetricToLogConfig {
    pub host_tag: Option<String>,
}

inventory::submit! {
    TransformDescription::new::<MetricToLogConfig>("metric_to_log")
}

impl GenerateConfig for MetricToLogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            host_tag: Some("host-tag".to_string()),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "metric_to_log")]
impl TransformConfig for MetricToLogConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Ok(Transform::function(MetricToLog::new(self.host_tag.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "metric_to_log"
    }
}

#[derive(Clone, Debug)]
pub struct MetricToLog {
    timestamp_key: String,
    host_tag: String,
}

impl MetricToLog {
    pub fn new(host_tag: Option<String>) -> Self {
        Self {
            timestamp_key: "timestamp".into(),
            host_tag: format!(
                "tags.{}",
                host_tag.unwrap_or_else(|| log_schema().host_key().to_string())
            ),
        }
    }
}

impl FunctionTransform for MetricToLog {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let metric = event.into_metric();
        emit!(MetricToLogEventProcessed);

        let retval = serde_json::to_value(&metric)
            .map_err(|error| emit!(MetricToLogFailedSerialize { error }))
            .ok()
            .and_then(|value| match value {
                Value::Object(object) => {
                    let mut log = LogEvent::default();

                    for (key, value) in object {
                        log.insert_flat(key, value);
                    }

                    let timestamp = log
                        .remove(&self.timestamp_key)
                        .and_then(|value| Conversion::Timestamp.convert(value.into_bytes()).ok())
                        .unwrap_or_else(|| event::Value::Timestamp(Utc::now()));
                    log.insert(&log_schema().timestamp_key(), timestamp);

                    if let Some(host) = log.remove_prune(&self.host_tag, true) {
                        log.insert(&log_schema().host_key(), host);
                    }

                    Some(log.into())
                }
                _ => None,
            });
        output.extend(retval.into_iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue, StatisticKind},
        Metric, Value,
    };
    use chrono::{offset::TimeZone, DateTime, Utc};
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MetricToLogConfig>();
    }

    fn do_transform(metric: Metric) -> Option<LogEvent> {
        let event = Event::Metric(metric);
        let mut transformer = MetricToLog::new(Some("host".into()));

        transformer
            .transform_one(event)
            .map(|event| event.into_log())
    }

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn tags() -> BTreeMap<String, String> {
        vec![
            ("host".to_owned(), "localhost".to_owned()),
            ("some_tag".to_owned(), "some_value".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn transform_counter() {
        let counter = Metric {
            name: "counter".into(),
            namespace: None,
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 1.0 },
        };

        let log = do_transform(counter).unwrap();
        let collected: Vec<_> = log.all_fields().collect();

        assert_eq!(
            collected,
            vec![
                (String::from("counter.value"), &Value::from(1.0)),
                (String::from("host"), &Value::from("localhost")),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("counter")),
                (String::from("tags.some_tag"), &Value::from("some_value")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_gauge() {
        let gauge = Metric {
            name: "gauge".into(),
            namespace: None,
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 1.0 },
        };

        let log = do_transform(gauge).unwrap();
        let collected: Vec<_> = log.all_fields().collect();

        assert_eq!(
            collected,
            vec![
                (String::from("gauge.value"), &Value::from(1.0)),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("gauge")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_set() {
        let set = Metric {
            name: "set".into(),
            namespace: None,
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Set {
                values: vec!["one".into(), "two".into()].into_iter().collect(),
            },
        };

        let log = do_transform(set).unwrap();
        let collected: Vec<_> = log.all_fields().collect();

        assert_eq!(
            collected,
            vec![
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("set")),
                (String::from("set.values[0]"), &Value::from("one")),
                (String::from("set.values[1]"), &Value::from("two")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_distribution() {
        let distro = Metric {
            name: "distro".into(),
            namespace: None,
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Distribution {
                values: vec![1.0, 2.0],
                sample_rates: vec![10, 20],
                statistic: StatisticKind::Histogram,
            },
        };

        let log = do_transform(distro).unwrap();
        let collected: Vec<_> = log.all_fields().collect();

        assert_eq!(
            collected,
            vec![
                (
                    String::from("distribution.sample_rates[0]"),
                    &Value::from(10)
                ),
                (
                    String::from("distribution.sample_rates[1]"),
                    &Value::from(20)
                ),
                (
                    String::from("distribution.statistic"),
                    &Value::from("histogram")
                ),
                (String::from("distribution.values[0]"), &Value::from(1.0)),
                (String::from("distribution.values[1]"), &Value::from(2.0)),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("distro")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_histogram() {
        let histo = Metric {
            name: "histo".into(),
            namespace: None,
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedHistogram {
                buckets: vec![1.0, 2.0],
                counts: vec![10, 20],
                count: 30,
                sum: 50.0,
            },
        };

        let log = do_transform(histo).unwrap();
        let collected: Vec<_> = log.all_fields().collect();

        assert_eq!(
            collected,
            vec![
                (
                    String::from("aggregated_histogram.buckets[0]"),
                    &Value::from(1.0)
                ),
                (
                    String::from("aggregated_histogram.buckets[1]"),
                    &Value::from(2.0)
                ),
                (String::from("aggregated_histogram.count"), &Value::from(30)),
                (
                    String::from("aggregated_histogram.counts[0]"),
                    &Value::from(10)
                ),
                (
                    String::from("aggregated_histogram.counts[1]"),
                    &Value::from(20)
                ),
                (String::from("aggregated_histogram.sum"), &Value::from(50.0)),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("histo")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_summary() {
        let summary = Metric {
            name: "summary".into(),
            namespace: None,
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedSummary {
                quantiles: vec![50.0, 90.0],
                values: vec![10.0, 20.0],
                count: 30,
                sum: 50.0,
            },
        };

        let log = do_transform(summary).unwrap();
        let collected: Vec<_> = log.all_fields().collect();

        assert_eq!(
            collected,
            vec![
                (String::from("aggregated_summary.count"), &Value::from(30)),
                (
                    String::from("aggregated_summary.quantiles[0]"),
                    &Value::from(50.0)
                ),
                (
                    String::from("aggregated_summary.quantiles[1]"),
                    &Value::from(90.0)
                ),
                (String::from("aggregated_summary.sum"), &Value::from(50.0)),
                (
                    String::from("aggregated_summary.values[0]"),
                    &Value::from(10.0)
                ),
                (
                    String::from("aggregated_summary.values[1]"),
                    &Value::from(20.0)
                ),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("summary")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
    }
}
