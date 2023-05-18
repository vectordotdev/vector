use chrono::Utc;
use codecs::MetricTagValues;
use lookup::lookup_v2::parse_value_path;
use lookup::{event_path, owned_value_path, path, PathPrefix};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vector_common::TimeZone;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;
use vrl::value::kind::Collection;
use vrl::value::Kind;

use crate::config::OutputId;
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Input, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::{self, Event, LogEvent, Metric},
    internal_events::MetricToLogSerializeError,
    schema::Definition,
    transforms::{FunctionTransform, OutputBuffer, Transform},
    types::Conversion,
};

/// Configuration for the `metric_to_log` transform.
#[configurable_component(transform("metric_to_log", "Convert metric events to log events."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct MetricToLogConfig {
    /// Name of the tag in the metric to use for the source host.
    ///
    /// If present, the value of the tag is set on the generated log event in the `host` field,
    /// where the field key uses the [global `host_key` option][global_log_schema_host_key].
    ///
    /// [global_log_schema_host_key]: https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key
    #[configurable(metadata(docs::examples = "host", docs::examples = "hostname"))]
    pub host_tag: Option<String>,

    /// The name of the time zone to apply to timestamp conversions that do not contain an explicit
    /// time zone.
    ///
    /// This overrides the [global `timezone`][global_timezone] option. The time zone name may be
    /// any name in the [TZ database][tz_database] or `local` to indicate system local time.
    ///
    /// [global_timezone]: https://vector.dev/docs/reference/configuration//global-options#timezone
    /// [tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    pub timezone: Option<TimeZone>,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,

    /// Controls how metric tag values are encoded.
    ///
    /// When set to `single`, only the last non-bare value of tags are displayed with the
    /// metric.  When set to `full`, all metric tags are exposed as separate assignments as
    /// described by [the `native_json` codec][vector_native_json].
    ///
    /// [vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
    #[serde(default)]
    pub metric_tag_values: MetricTagValues,
}

impl MetricToLogConfig {
    pub fn build_transform(&self, context: &TransformContext) -> MetricToLog {
        MetricToLog::new(
            self.host_tag.as_deref(),
            self.timezone.unwrap_or_else(|| context.globals.timezone()),
            context.log_namespace(self.log_namespace),
            self.metric_tag_values,
        )
    }
}

impl GenerateConfig for MetricToLogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            host_tag: Some("host-tag".to_string()),
            timezone: None,
            log_namespace: None,
            metric_tag_values: MetricTagValues::Single,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "metric_to_log")]
impl TransformConfig for MetricToLogConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(self.build_transform(context)))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn outputs(
        &self,
        _: enrichment::TableRegistry,
        input_definitions: &[(OutputId, Definition)],
        global_log_namespace: LogNamespace,
    ) -> Vec<TransformOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let mut schema_definition =
            Definition::default_for_namespace(&BTreeSet::from([log_namespace]))
                .with_event_field(&owned_value_path!("name"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("namespace"),
                    Kind::bytes().or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("tags"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None,
                )
                .with_event_field(&owned_value_path!("kind"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("counter"),
                    Kind::object(Collection::empty().with_known("value", Kind::float()))
                        .or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("gauge"),
                    Kind::object(Collection::empty().with_known("value", Kind::float()))
                        .or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("set"),
                    Kind::object(Collection::empty().with_known(
                        "values",
                        Kind::array(Collection::empty().with_unknown(Kind::bytes())),
                    ))
                    .or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("distribution"),
                    Kind::object(
                        Collection::empty()
                            .with_known(
                                "samples",
                                Kind::array(
                                    Collection::empty().with_unknown(Kind::object(
                                        Collection::empty()
                                            .with_known("value", Kind::float())
                                            .with_known("rate", Kind::integer()),
                                    )),
                                ),
                            )
                            .with_known("statistic", Kind::bytes()),
                    )
                    .or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("aggregated_histogram"),
                    Kind::object(
                        Collection::empty()
                            .with_known(
                                "buckets",
                                Kind::array(
                                    Collection::empty().with_unknown(Kind::object(
                                        Collection::empty()
                                            .with_known("upper_limit", Kind::float())
                                            .with_known("count", Kind::integer()),
                                    )),
                                ),
                            )
                            .with_known("count", Kind::integer())
                            .with_known("sum", Kind::float()),
                    )
                    .or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("aggregated_summary"),
                    Kind::object(
                        Collection::empty()
                            .with_known(
                                "quantiles",
                                Kind::array(
                                    Collection::empty().with_unknown(Kind::object(
                                        Collection::empty()
                                            .with_known("quantile", Kind::float())
                                            .with_known("value", Kind::float()),
                                    )),
                                ),
                            )
                            .with_known("count", Kind::integer())
                            .with_known("sum", Kind::float()),
                    )
                    .or_undefined(),
                    None,
                )
                .with_event_field(
                    &owned_value_path!("sketch"),
                    Kind::any().or_undefined(),
                    None,
                );

        match log_namespace {
            LogNamespace::Vector => {
                // from serializing the Metric (Legacy moves it to another field)
                schema_definition = schema_definition.with_event_field(
                    &owned_value_path!("timestamp"),
                    Kind::bytes().or_undefined(),
                    None,
                );

                // This is added as a "marker" field to determine which namespace is being used at runtime.
                // This is normally handled automatically by sources, but this is a special case.
                schema_definition = schema_definition.with_metadata_field(
                    &owned_value_path!("vector"),
                    Kind::object(Collection::empty()),
                    None,
                );
            }
            LogNamespace::Legacy => {
                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    schema_definition =
                        schema_definition.with_event_field(timestamp_key, Kind::timestamp(), None);
                }

                schema_definition = schema_definition.with_event_field(
                    &parse_value_path(log_schema().host_key()).expect("valid host key"),
                    Kind::bytes().or_undefined(),
                    None,
                );
            }
        }

        vec![TransformOutput::new(
            DataType::Log,
            input_definitions
                .iter()
                .map(|(output, _)| (output.clone(), schema_definition.clone()))
                .collect(),
        )]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
pub struct MetricToLog {
    host_tag: String,
    timezone: TimeZone,
    log_namespace: LogNamespace,
    tag_values: MetricTagValues,
}

impl MetricToLog {
    pub fn new(
        host_tag: Option<&str>,
        timezone: TimeZone,
        log_namespace: LogNamespace,
        tag_values: MetricTagValues,
    ) -> Self {
        Self {
            host_tag: format!(
                "tags.{}",
                host_tag.unwrap_or_else(|| log_schema().host_key())
            ),
            timezone,
            log_namespace,
            tag_values,
        }
    }

    pub fn transform_one(&self, mut metric: Metric) -> Option<LogEvent> {
        if self.tag_values == MetricTagValues::Single {
            metric.reduce_tags_to_single();
        }
        serde_json::to_value(&metric)
            .map_err(|error| emit!(MetricToLogSerializeError { error }))
            .ok()
            .and_then(|value| match value {
                Value::Object(object) => {
                    let (_, _, metadata) = metric.into_parts();
                    let mut log = LogEvent::new_with_metadata(metadata);

                    // converting all fields from serde `Value` to Vector `Value`
                    for (key, value) in object {
                        log.insert(event_path!(&key), value);
                    }

                    if self.log_namespace == LogNamespace::Legacy {
                        // "Vector" namespace just leaves the `timestamp` in place.

                        let timestamp = log
                            .remove(event_path!("timestamp"))
                            .and_then(|value| {
                                Conversion::Timestamp(self.timezone)
                                    .convert(value.coerce_to_bytes())
                                    .ok()
                            })
                            .unwrap_or_else(|| event::Value::Timestamp(Utc::now()));

                        if let Some(timestamp_key) = log_schema().timestamp_key() {
                            log.insert((PathPrefix::Event, timestamp_key), timestamp);
                        }

                        if let Some(host) = log.remove_prune(self.host_tag.as_str(), true) {
                            log.insert(log_schema().host_key(), host);
                        }
                    }
                    if self.log_namespace == LogNamespace::Vector {
                        // Create vector metadata since this is used as a marker to see which namespace is used at runtime.
                        // This can be removed once metrics support namespacing.
                        log.insert(
                            (PathPrefix::Metadata, path!("vector")),
                            vrl::value::Value::Object(BTreeMap::new()),
                        );
                    }
                    Some(log)
                }
                _ => None,
            })
    }
}

impl FunctionTransform for MetricToLog {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let retval: Option<Event> = self
            .transform_one(event.into_metric())
            .map(|log| log.into());
        output.extend(retval.into_iter())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use futures::executor::block_on;
    use proptest::prelude::*;
    use similar_asserts::assert_eq;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_core::metric_tags;

    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricTags, MetricValue, StatisticKind, TagValue, TagValueSet},
        Metric, Value,
    };
    use crate::test_util::{components::assert_transform_compliance, random_string};
    use crate::transforms::test::create_topology;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MetricToLogConfig>();
    }

    async fn do_transform(metric: Metric) -> Option<LogEvent> {
        assert_transform_compliance(async move {
            let config = MetricToLogConfig {
                host_tag: Some("host".into()),
                timezone: None,
                log_namespace: Some(false),
                ..Default::default()
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            tx.send(metric.into()).await.unwrap();

            let result = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);

            result
        })
        .await
        .map(|e| e.into_log())
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }

    fn tags() -> MetricTags {
        metric_tags! {
            "host" => "localhost",
            "some_tag" => "some_value",
        }
    }

    #[tokio::test]
    async fn transform_counter() {
        let counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));
        let mut metadata = counter.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let log = do_transform(counter).await.unwrap();
        let collected: Vec<_> = log.all_fields().unwrap().collect();

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
        assert_eq!(log.metadata(), &metadata);
    }

    #[tokio::test]
    async fn transform_gauge() {
        let gauge = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        )
        .with_timestamp(Some(ts()));
        let mut metadata = gauge.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let log = do_transform(gauge).await.unwrap();
        let collected: Vec<_> = log.all_fields().unwrap().collect();

        assert_eq!(
            collected,
            vec![
                (String::from("gauge.value"), &Value::from(1.0)),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("gauge")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
        assert_eq!(log.metadata(), &metadata);
    }

    #[tokio::test]
    async fn transform_set() {
        let set = Metric::new(
            "set",
            MetricKind::Absolute,
            MetricValue::Set {
                values: vec!["one".into(), "two".into()].into_iter().collect(),
            },
        )
        .with_timestamp(Some(ts()));
        let mut metadata = set.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let log = do_transform(set).await.unwrap();
        let collected: Vec<_> = log.all_fields().unwrap().collect();

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
        assert_eq!(log.metadata(), &metadata);
    }

    #[tokio::test]
    async fn transform_distribution() {
        let distro = Metric::new(
            "distro",
            MetricKind::Absolute,
            MetricValue::Distribution {
                samples: vector_core::samples![1.0 => 10, 2.0 => 20],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_timestamp(Some(ts()));
        let mut metadata = distro.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let log = do_transform(distro).await.unwrap();
        let collected: Vec<_> = log.all_fields().unwrap().collect();

        assert_eq!(
            collected,
            vec![
                (
                    String::from("distribution.samples[0].rate"),
                    &Value::from(10)
                ),
                (
                    String::from("distribution.samples[0].value"),
                    &Value::from(1.0)
                ),
                (
                    String::from("distribution.samples[1].rate"),
                    &Value::from(20)
                ),
                (
                    String::from("distribution.samples[1].value"),
                    &Value::from(2.0)
                ),
                (
                    String::from("distribution.statistic"),
                    &Value::from("histogram")
                ),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("distro")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
        assert_eq!(log.metadata(), &metadata);
    }

    #[tokio::test]
    async fn transform_histogram() {
        let histo = Metric::new(
            "histo",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vector_core::buckets![1.0 => 10, 2.0 => 20],
                count: 30,
                sum: 50.0,
            },
        )
        .with_timestamp(Some(ts()));
        let mut metadata = histo.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let log = do_transform(histo).await.unwrap();
        let collected: Vec<_> = log.all_fields().unwrap().collect();

        assert_eq!(
            collected,
            vec![
                (
                    String::from("aggregated_histogram.buckets[0].count"),
                    &Value::from(10)
                ),
                (
                    String::from("aggregated_histogram.buckets[0].upper_limit"),
                    &Value::from(1.0)
                ),
                (
                    String::from("aggregated_histogram.buckets[1].count"),
                    &Value::from(20)
                ),
                (
                    String::from("aggregated_histogram.buckets[1].upper_limit"),
                    &Value::from(2.0)
                ),
                (String::from("aggregated_histogram.count"), &Value::from(30)),
                (String::from("aggregated_histogram.sum"), &Value::from(50.0)),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("histo")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
        assert_eq!(log.metadata(), &metadata);
    }

    #[tokio::test]
    async fn transform_summary() {
        let summary = Metric::new(
            "summary",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vector_core::quantiles![50.0 => 10.0, 90.0 => 20.0],
                count: 30,
                sum: 50.0,
            },
        )
        .with_timestamp(Some(ts()));
        let mut metadata = summary.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let log = do_transform(summary).await.unwrap();
        let collected: Vec<_> = log.all_fields().unwrap().collect();

        assert_eq!(
            collected,
            vec![
                (String::from("aggregated_summary.count"), &Value::from(30)),
                (
                    String::from("aggregated_summary.quantiles[0].quantile"),
                    &Value::from(50.0)
                ),
                (
                    String::from("aggregated_summary.quantiles[0].value"),
                    &Value::from(10.0)
                ),
                (
                    String::from("aggregated_summary.quantiles[1].quantile"),
                    &Value::from(90.0)
                ),
                (
                    String::from("aggregated_summary.quantiles[1].value"),
                    &Value::from(20.0)
                ),
                (String::from("aggregated_summary.sum"), &Value::from(50.0)),
                (String::from("kind"), &Value::from("absolute")),
                (String::from("name"), &Value::from("summary")),
                (String::from("timestamp"), &Value::from(ts())),
            ]
        );
        assert_eq!(log.metadata(), &metadata);
    }

    // Test the encoding of tag values with the `metric_tag_values` flag.
    proptest! {
        #[test]
        fn transform_tag_single_encoding(values: TagValueSet) {
            let name = random_string(16);
            let tags = block_on(transform_tags(
                MetricTagValues::Single,
                values.iter()
                    .map(|value| (name.clone(), TagValue::from(value.map(String::from))))
                    .collect(),
            ));
            // The resulting tag must be either a single string value or not present.
            let value = values.into_single().map(|value| Value::Bytes(value.into()));
            assert_eq!(tags.get(&*name), value.as_ref());
        }

        #[test]
        fn transform_tag_full_encoding(values: TagValueSet) {
            let name = random_string(16);
            let tags = block_on(transform_tags(
                MetricTagValues::Full,
                values.iter()
                    .map(|value| (name.clone(), TagValue::from(value.map(String::from))))
                    .collect(),
            ));
            let tag = tags.get(&*name);
            match values.len() {
                // Empty tag set => missing tag
                0 => assert_eq!(tag, None),
                // Single value tag => scalar value
                1 => assert_eq!(tag, Some(&tag_to_value(values.into_iter().next().unwrap()))),
                // Multi-valued tag => array value
                _ => assert_eq!(tag, Some(&Value::Array(values.into_iter().map(tag_to_value).collect()))),
            }
        }
    }

    fn tag_to_value(tag: TagValue) -> Value {
        tag.into_option().into()
    }

    async fn transform_tags(metric_tag_values: MetricTagValues, tags: MetricTags) -> Value {
        let counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(tags))
        .with_timestamp(Some(ts()));

        let mut output = OutputBuffer::with_capacity(1);

        MetricToLogConfig {
            metric_tag_values,
            ..Default::default()
        }
        .build_transform(&TransformContext::default())
        .transform(&mut output, counter.into());

        assert_eq!(output.len(), 1);
        output.into_events().next().unwrap().into_log()["tags"].clone()
    }
}
