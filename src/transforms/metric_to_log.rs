use crate::{
    config::{
        log_schema, DataType, GenerateConfig, GlobalOptions, TransformConfig, TransformDescription,
    },
    event::{self, Event, LogEvent, LookupBuf},
    internal_events::MetricToLogFailedSerialize,
    transforms::{FunctionTransform, Transform},
    types::Conversion,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::TimeZone;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetricToLogConfig {
    pub host_tag: Option<LookupBuf>,
    pub timezone: Option<TimeZone>,
}

inventory::submit! {
    TransformDescription::new::<MetricToLogConfig>("metric_to_log")
}

impl GenerateConfig for MetricToLogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            host_tag: Some(LookupBuf::from("host-tag")),
            timezone: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "metric_to_log")]
impl TransformConfig for MetricToLogConfig {
    async fn build(&self, globals: &GlobalOptions) -> crate::Result<Transform> {
        Ok(Transform::function(MetricToLog::new(
            self.host_tag.clone(),
            self.timezone.unwrap_or(globals.timezone),
        )))
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
    timestamp_key: LookupBuf,
    host_tag: LookupBuf,
    timezone: TimeZone,
}

impl MetricToLog {
    pub fn new(host_tag: Option<LookupBuf>, timezone: TimeZone) -> Self {
        let host_tag = host_tag.unwrap_or_else(|| log_schema().host_key().clone());
        let mut tag_lookup = LookupBuf::from("tags");
        tag_lookup.extend(host_tag);
        Self {
            timestamp_key: "timestamp".into(),
            host_tag: tag_lookup,
            timezone,
        }
    }
}

impl FunctionTransform for MetricToLog {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let metric = event.into_metric();

        let retval = serde_json::to_value(&metric)
            .map_err(|error| emit!(MetricToLogFailedSerialize { error }))
            .ok()
            .and_then(|value| match value {
                Value::Object(object) => {
                    let mut log = LogEvent::default();

                    for (key, value) in object {
                        log.insert(LookupBuf::from(key), value);
                    }

                    let timestamp = log
                        .remove(&self.timestamp_key, false)
                        .and_then(|value| {
                            Conversion::Timestamp(self.timezone)
                                .convert(value.clone_into_bytes())
                                .ok()
                        })
                        .unwrap_or_else(|| event::Value::Timestamp(Utc::now()));
                    log.insert(log_schema().timestamp_key().clone(), timestamp);

                    if let Some(host) = log.remove(&self.host_tag, true) {
                        log.insert(log_schema().host_key().clone(), host);
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
        Lookup, Metric, Value,
    };
    use chrono::{offset::TimeZone, DateTime, Utc};
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MetricToLogConfig>();
    }

    fn do_transform(metric: Metric) -> Option<LogEvent> {
        let event = Event::Metric(metric);
        let mut transformer = MetricToLog::new(Some("host".into()), Default::default());

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
        let counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        let log = do_transform(counter).unwrap();
        let collected: Vec<_> = log.pairs(true).collect();

        assert_eq!(
            collected,
            vec![
                (
                    Lookup::from_str("counter.value").unwrap(),
                    &Value::from(1.0)
                ),
                (Lookup::from_str("host").unwrap(), &Value::from("localhost")),
                (Lookup::from_str("kind").unwrap(), &Value::from("absolute")),
                (Lookup::from_str("name").unwrap(), &Value::from("counter")),
                (
                    Lookup::from_str("tags.some_tag").unwrap(),
                    &Value::from("some_value")
                ),
                (Lookup::from_str("timestamp").unwrap(), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_gauge() {
        let gauge = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        )
        .with_timestamp(Some(ts()));

        let log = do_transform(gauge).unwrap();
        let collected: Vec<_> = log.pairs(true).collect();

        assert_eq!(
            collected,
            vec![
                (Lookup::from_str("gauge.value").unwrap(), &Value::from(1.0)),
                (Lookup::from_str("kind").unwrap(), &Value::from("absolute")),
                (Lookup::from_str("name").unwrap(), &Value::from("gauge")),
                (Lookup::from_str("timestamp").unwrap(), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_set() {
        let set = Metric::new(
            "set",
            MetricKind::Absolute,
            MetricValue::Set {
                values: vec!["one".into(), "two".into()].into_iter().collect(),
            },
        )
        .with_timestamp(Some(ts()));

        let log = do_transform(set).unwrap();
        let collected: Vec<_> = log.pairs(true).collect();

        assert_eq!(
            collected,
            vec![
                (Lookup::from_str("kind").unwrap(), &Value::from("absolute")),
                (Lookup::from_str("name").unwrap(), &Value::from("set")),
                (
                    Lookup::from_str("set.values[0]").unwrap(),
                    &Value::from("one")
                ),
                (
                    Lookup::from_str("set.values[1]").unwrap(),
                    &Value::from("two")
                ),
                (Lookup::from_str("timestamp").unwrap(), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_distribution() {
        let distro = Metric::new(
            "distro",
            MetricKind::Absolute,
            MetricValue::Distribution {
                samples: crate::samples![1.0 => 10, 2.0 => 20],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_timestamp(Some(ts()));

        let log = do_transform(distro).unwrap();
        let collected: Vec<_> = log.pairs(true).collect();

        assert_eq!(
            collected,
            vec![
                (
                    Lookup::from_str("distribution.samples[0].rate").unwrap(),
                    &Value::from(10)
                ),
                (
                    Lookup::from_str("distribution.samples[0].value").unwrap(),
                    &Value::from(1.0)
                ),
                (
                    Lookup::from_str("distribution.samples[1].rate").unwrap(),
                    &Value::from(20)
                ),
                (
                    Lookup::from_str("distribution.samples[1].value").unwrap(),
                    &Value::from(2.0)
                ),
                (
                    Lookup::from_str("distribution.statistic").unwrap(),
                    &Value::from("histogram")
                ),
                (Lookup::from_str("kind").unwrap(), &Value::from("absolute")),
                (Lookup::from_str("name").unwrap(), &Value::from("distro")),
                (Lookup::from_str("timestamp").unwrap(), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_histogram() {
        let histo = Metric::new(
            "histo",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![1.0 => 10, 2.0 => 20],
                count: 30,
                sum: 50.0,
            },
        )
        .with_timestamp(Some(ts()));

        let log = do_transform(histo).unwrap();
        let collected: Vec<_> = log.pairs(true).collect();

        assert_eq!(
            collected,
            vec![
                (
                    Lookup::from_str("aggregated_histogram.buckets[0].count").unwrap(),
                    &Value::from(10)
                ),
                (
                    Lookup::from_str("aggregated_histogram.buckets[0].upper_limit").unwrap(),
                    &Value::from(1.0)
                ),
                (
                    Lookup::from_str("aggregated_histogram.buckets[1].count").unwrap(),
                    &Value::from(20)
                ),
                (
                    Lookup::from_str("aggregated_histogram.buckets[1].upper_limit").unwrap(),
                    &Value::from(2.0)
                ),
                (
                    Lookup::from_str("aggregated_histogram.count").unwrap(),
                    &Value::from(30)
                ),
                (
                    Lookup::from_str("aggregated_histogram.sum").unwrap(),
                    &Value::from(50.0)
                ),
                (Lookup::from_str("kind").unwrap(), &Value::from("absolute")),
                (Lookup::from_str("name").unwrap(), &Value::from("histo")),
                (Lookup::from_str("timestamp").unwrap(), &Value::from(ts())),
            ]
        );
    }

    #[test]
    fn transform_summary() {
        let summary = Metric::new(
            "summary",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: crate::quantiles![50.0 => 10.0, 90.0 => 20.0],
                count: 30,
                sum: 50.0,
            },
        )
        .with_timestamp(Some(ts()));

        let log = do_transform(summary).unwrap();
        let collected: Vec<_> = log.pairs(true).collect();

        assert_eq!(
            collected,
            vec![
                (
                    Lookup::from_str("aggregated_summary.quantiles[0].upper_limit").unwrap(),
                    &Value::from(50.0)
                ),
                (
                    Lookup::from_str("aggregated_summary.quantiles[0].value").unwrap(),
                    &Value::from(10.0)
                ),
                (
                    Lookup::from_str("aggregated_summary.quantiles[1].upper_limit").unwrap(),
                    &Value::from(90.0)
                ),
                (
                    Lookup::from_str("aggregated_summary.quantiles[1].value").unwrap(),
                    &Value::from(20.0)
                ),
                (
                    Lookup::from_str("aggregated_summary.sum").unwrap(),
                    &Value::from(50.0)
                ),
                (Lookup::from_str("kind").unwrap(), &Value::from("absolute")),
                (Lookup::from_str("name").unwrap(), &Value::from("summary")),
                (Lookup::from_str("timestamp").unwrap(), &Value::from(ts())),
            ]
        );
    }
}
