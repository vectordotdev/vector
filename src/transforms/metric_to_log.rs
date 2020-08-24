use super::Transform;
use crate::{
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
    event::{
        self,
        metric::{MetricKind, MetricValue, StatisticKind},
        Event, LogEvent, Metric, Value,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MetricToLogConfig {
    pub host_tag: Option<String>,
}

inventory::submit! {
    TransformDescription::new_without_default::<MetricToLogConfig>("metric_to_log")
}

#[typetag::serde(name = "metric_to_log")]
impl TransformConfig for MetricToLogConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(MetricToLog::new(self.host_tag.clone())))
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

pub struct MetricToLog {
    host_tag: String,
}

impl MetricToLog {
    pub fn new(host_tag: Option<String>) -> Self {
        Self {
            host_tag: host_tag.unwrap_or_else(|| event::log_schema().host_key().to_string()),
        }
    }
}

impl Transform for MetricToLog {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let Metric {
            name,
            timestamp,
            tags,
            kind,
            value,
        } = event.into_metric();

        let mut log = LogEvent::default();
        log.insert_flat("name", name);
        log.insert_flat("kind", kind);

        if let Some(timestamp) = timestamp {
            log.insert(&event::log_schema().timestamp_key(), timestamp);
        }

        if let Some(mut tags) = tags {
            if let Some(host) = tags.remove(&self.host_tag) {
                log.insert(&self.host_tag, host);
            }
            log.insert_flat("tags", tags);
        }

        match value {
            MetricValue::Counter { value } => {
                log.insert("counter.value", value);
            }
            MetricValue::Gauge { value } => {
                log.insert("gauge.value", value);
            }
            MetricValue::Set { values } => {
                log.insert(
                    "set.values",
                    values.into_iter().map(Value::from).collect::<Vec<_>>(),
                );
            }
            MetricValue::Distribution {
                values,
                sample_rates,
                statistic,
            } => {
                let values = values.into_iter().map(Value::from).collect::<Vec<_>>();
                let sample_rates = sample_rates
                    .into_iter()
                    .map(|i| Value::from(i as i64))
                    .collect::<Vec<_>>();

                let mut map = BTreeMap::new();
                map.insert("values".to_string(), Value::from(values));
                map.insert("sample_rates".to_string(), Value::from(sample_rates));
                map.insert("statistic".to_string(), Value::from(statistic));
                log.insert_flat("distribution", map);
            }
            MetricValue::AggregatedHistogram {
                buckets,
                counts,
                count,
                sum,
            } => {
                let buckets = buckets.into_iter().map(Value::from).collect::<Vec<_>>();
                let counts = counts
                    .into_iter()
                    .map(|i| Value::from(i as i64))
                    .collect::<Vec<_>>();

                let mut map = BTreeMap::new();
                map.insert("buckets".to_string(), Value::from(buckets));
                map.insert("counts".to_string(), Value::from(counts));
                map.insert("count".to_string(), Value::from(count as i64));
                map.insert("sum".to_string(), Value::from(sum));
                log.insert_flat("aggregated_histogram", map);
            }
            MetricValue::AggregatedSummary {
                quantiles,
                values,
                count,
                sum,
            } => {
                let quantiles = quantiles.into_iter().map(Value::from).collect::<Vec<_>>();
                let values = values.into_iter().map(Value::from).collect::<Vec<_>>();

                let mut map = BTreeMap::new();
                map.insert("quantiles".to_string(), Value::from(quantiles));
                map.insert("values".to_string(), Value::from(values));
                map.insert("count".to_string(), Value::from(count as i64));
                map.insert("sum".to_string(), Value::from(sum));
                log.insert_flat("aggregated_summary", map);
            }
        }

        Some(Event::Log(log))
    }
}

impl From<MetricKind> for Value {
    fn from(kind: MetricKind) -> Self {
        match kind {
            MetricKind::Incremental => "incremental",
            MetricKind::Absolute => "absolute",
        }
        .into()
    }
}

impl From<BTreeMap<String, String>> for Value {
    fn from(value: BTreeMap<String, String>) -> Self {
        let value = value
            .into_iter()
            .map(|(k, v)| (k, Value::from(v)))
            .collect::<BTreeMap<_, _>>();
        Value::Map(value)
    }
}

impl From<StatisticKind> for Value {
    fn from(kind: StatisticKind) -> Self {
        match kind {
            StatisticKind::Histogram => "histogram",
            StatisticKind::Summary => "summary",
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{offset::TimeZone, DateTime, Utc};

    fn do_transform(metric: Metric) -> Option<LogEvent> {
        let event = Event::Metric(metric);
        let mut transformer = toml::from_str::<MetricToLogConfig>(
            r#"
                host_tag = "host"
            "#,
        )
        .unwrap()
        .build(TransformContext::new_test())
        .unwrap();

        transformer.transform(event).map(|event| event.into_log())
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
            timestamp: None,
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
            ]
        );
    }

    #[test]
    fn transform_set() {
        let set = Metric {
            name: "set".into(),
            timestamp: None,
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
            ]
        );
    }

    #[test]
    fn transform_distribution() {
        let distro = Metric {
            name: "distro".into(),
            timestamp: None,
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
            ]
        );
    }

    #[test]
    fn transform_histogram() {
        let histo = Metric {
            name: "histo".into(),
            timestamp: None,
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
            ]
        );
    }

    #[test]
    fn transform_summary() {
        let summary = Metric {
            name: "summary".into(),
            timestamp: None,
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
            ]
        );
    }
}
