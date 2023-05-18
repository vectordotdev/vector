use std::{collections::HashMap, num::ParseFloatError};

use chrono::Utc;
use indexmap::IndexMap;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::{
        metric::{Metric, MetricKind, MetricTags, MetricValue, StatisticKind, TagValue},
        Event, Value,
    },
    internal_events::{
        LogToMetricFieldNullError, LogToMetricParseFloatError, ParserMissingFieldError, DROP_EVENT,
    },
    schema,
    template::{Template, TemplateRenderingError},
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `log_to_metric` transform.
#[configurable_component(transform("log_to_metric", "Convert log events to metric events."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LogToMetricConfig {
    /// A list of metrics to generate.
    pub metrics: Vec<MetricConfig>,
}

/// Specification of a counter derived from a log event.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct CounterConfig {
    /// Increments the counter by the value in `field`, instead of only by `1`.
    #[serde(default = "default_increment_by_value")]
    pub increment_by_value: bool,

    #[configurable(derived)]
    #[serde(default = "default_kind")]
    pub kind: MetricKind,
}

/// Specification of a metric derived from a log event.
// TODO: While we're resolving the schema for this enum somewhat reasonably (in
// `generate-components-docs.rb`), we have a problem where an overlapping field (overlap between two
// or more of the subschemas) takes the details of the last subschema to be iterated over that
// contains that field, such that, for example, the `Summary` variant below is overriding the
// description for almost all of the fields because they're shared across all of the variants.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct MetricConfig {
    /// Name of the field in the event to generate the metric.
    pub field: Template,

    /// Overrides the name of the counter.
    ///
    /// If not specified, `field` is used as the name of the metric.
    pub name: Option<Template>,

    /// Sets the namespace for the metric.
    pub namespace: Option<Template>,

    /// Tags to apply to the metric.
    #[configurable(metadata(docs::additional_props_description = "A metric tag."))]
    pub tags: Option<IndexMap<String, TagConfig>>,

    #[configurable(derived)]
    #[serde(flatten)]
    pub metric: MetricTypeConfig,
}

/// Specification of the value of a created tag.
///
/// This may be a single value, a `null` for a bare tag, or an array of either.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum TagConfig {
    /// A single tag value.
    Plain(Option<Template>),

    /// An array of values to give to the same tag name.
    Multi(Vec<Option<Template>>),
}

/// Specification of the type of an individual metric, and any associated data.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of metric to create."))]
pub enum MetricTypeConfig {
    /// A counter.
    Counter(CounterConfig),

    /// A histogram.
    Histogram,

    /// A gauge.
    Gauge,

    /// A set.
    Set,

    /// A summary.
    Summary,
}

impl MetricConfig {
    fn field(&self) -> &str {
        self.field.get_ref()
    }
}

const fn default_increment_by_value() -> bool {
    false
}

const fn default_kind() -> MetricKind {
    MetricKind::Incremental
}

#[derive(Debug, Clone)]
pub struct LogToMetric {
    config: LogToMetricConfig,
}

impl GenerateConfig for LogToMetricConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            metrics: vec![MetricConfig {
                field: "field_name".try_into().expect("Fixed template"),
                name: None,
                namespace: None,
                tags: None,
                metric: MetricTypeConfig::Counter(CounterConfig {
                    increment_by_value: false,
                    kind: MetricKind::Incremental,
                }),
            }],
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "log_to_metric")]
impl TransformConfig for LogToMetricConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(LogToMetric::new(self.clone())))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: enrichment::TableRegistry,
        _: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        // Converting the log to a metric means we lose all incoming `Definition`s.
        vec![TransformOutput::new(DataType::Metric, HashMap::new())]
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

impl LogToMetric {
    pub const fn new(config: LogToMetricConfig) -> Self {
        LogToMetric { config }
    }
}

enum TransformError {
    FieldNotFound {
        field: String,
    },
    FieldNull {
        field: String,
    },
    TemplateRenderingError(TemplateRenderingError),
    ParseFloatError {
        field: String,
        error: ParseFloatError,
    },
}

fn render_template(template: &Template, event: &Event) -> Result<String, TransformError> {
    template
        .render_string(event)
        .map_err(TransformError::TemplateRenderingError)
}

fn render_tags(
    tags: &Option<IndexMap<String, TagConfig>>,
    event: &Event,
) -> Result<Option<MetricTags>, TransformError> {
    Ok(match tags {
        None => None,
        Some(tags) => {
            let mut result = MetricTags::default();
            for (name, config) in tags {
                match config {
                    TagConfig::Plain(template) => {
                        render_tag_into(event, name, template, &mut result)?
                    }
                    TagConfig::Multi(vec) => {
                        for template in vec {
                            render_tag_into(event, name, template, &mut result)?;
                        }
                    }
                }
            }
            result.as_option()
        }
    })
}

fn render_tag_into(
    event: &Event,
    name: &str,
    template: &Option<Template>,
    result: &mut MetricTags,
) -> Result<(), TransformError> {
    let value = match template {
        None => TagValue::Bare,
        Some(template) => match render_template(template, event) {
            Ok(result) => TagValue::Value(result),
            Err(TransformError::TemplateRenderingError(error)) => {
                emit!(crate::internal_events::TemplateRenderingError {
                    error,
                    drop_event: false,
                    field: Some(name),
                });
                return Ok(());
            }
            Err(other) => return Err(other),
        },
    };
    result.insert(name.to_string(), value);
    Ok(())
}

fn to_metric(config: &MetricConfig, event: &Event) -> Result<Metric, TransformError> {
    let log = event.as_log();

    let timestamp = log
        .get_timestamp()
        .and_then(Value::as_timestamp)
        .cloned()
        .or_else(|| Some(Utc::now()));
    let metadata = event.metadata().clone();

    let field = config.field();

    let value = match log.get(field) {
        None => Err(TransformError::FieldNotFound {
            field: field.to_string(),
        }),
        Some(Value::Null) => Err(TransformError::FieldNull {
            field: field.to_string(),
        }),
        Some(value) => Ok(value),
    }?;

    let name = config.name.as_ref().unwrap_or(&config.field);
    let name = render_template(name, event)?;

    let namespace = config.namespace.as_ref();
    let namespace = namespace
        .map(|namespace| render_template(namespace, event))
        .transpose()?;

    let tags = render_tags(&config.tags, event)?;

    let (kind, value) = match &config.metric {
        MetricTypeConfig::Counter(counter) => {
            let value = if counter.increment_by_value {
                value.to_string_lossy().parse().map_err(|error| {
                    TransformError::ParseFloatError {
                        field: config.field.get_ref().to_owned(),
                        error,
                    }
                })?
            } else {
                1.0
            };

            (counter.kind, MetricValue::Counter { value })
        }
        MetricTypeConfig::Histogram => {
            let value = value.to_string_lossy().parse().map_err(|error| {
                TransformError::ParseFloatError {
                    field: field.to_string(),
                    error,
                }
            })?;

            (
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_core::samples![value => 1],
                    statistic: StatisticKind::Histogram,
                },
            )
        }
        MetricTypeConfig::Summary => {
            let value = value.to_string_lossy().parse().map_err(|error| {
                TransformError::ParseFloatError {
                    field: field.to_string(),
                    error,
                }
            })?;

            (
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_core::samples![value => 1],
                    statistic: StatisticKind::Summary,
                },
            )
        }
        MetricTypeConfig::Gauge => {
            let value = value.to_string_lossy().parse().map_err(|error| {
                TransformError::ParseFloatError {
                    field: field.to_string(),
                    error,
                }
            })?;

            (MetricKind::Absolute, MetricValue::Gauge { value })
        }
        MetricTypeConfig::Set => {
            let value = value.to_string_lossy().into_owned();

            (
                MetricKind::Incremental,
                MetricValue::Set {
                    values: std::iter::once(value).collect(),
                },
            )
        }
    };
    Ok(Metric::new_with_metadata(name, kind, value, metadata)
        .with_namespace(namespace)
        .with_tags(tags)
        .with_timestamp(timestamp))
}

impl FunctionTransform for LogToMetric {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        // Metrics are "all or none" for a specific log. If a single fails, none are produced.
        let mut buffer = Vec::with_capacity(self.config.metrics.len());

        for config in self.config.metrics.iter() {
            match to_metric(config, &event) {
                Ok(metric) => {
                    buffer.push(Event::Metric(metric));
                }
                Err(err) => {
                    match err {
                        TransformError::FieldNull { field } => emit!(LogToMetricFieldNullError {
                            field: field.as_ref()
                        }),
                        TransformError::FieldNotFound { field } => {
                            emit!(ParserMissingFieldError::<DROP_EVENT> {
                                field: field.as_ref()
                            })
                        }
                        TransformError::ParseFloatError { field, error } => {
                            emit!(LogToMetricParseFloatError {
                                field: field.as_ref(),
                                error
                            })
                        }
                        TransformError::TemplateRenderingError(error) => {
                            emit!(crate::internal_events::TemplateRenderingError {
                                error,
                                drop_event: true,
                                field: None,
                            })
                        }
                    };
                    // early return to prevent the partial buffer from being sent
                    return;
                }
            }
        }

        // Metric generation was successful, publish them all.
        for event in buffer {
            output.push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use lookup::PathPrefix;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_core::metric_tags;

    use super::*;
    use crate::test_util::components::assert_transform_compliance;
    use crate::transforms::test::create_topology;
    use crate::{
        config::log_schema,
        event::{
            metric::{Metric, MetricKind, MetricValue, StatisticKind},
            Event, LogEvent,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LogToMetricConfig>();
    }

    fn parse_config(s: &str) -> LogToMetricConfig {
        toml::from_str(s).unwrap()
    }

    fn parse_yaml_config(s: &str) -> LogToMetricConfig {
        serde_yaml::from_str(s).unwrap()
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }

    fn create_event(key: &str, value: impl Into<Value> + std::fmt::Debug) -> Event {
        let mut log = Event::Log(LogEvent::from("i am a log"));
        log.as_mut_log().insert(key, value);
        log.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts(),
        );
        log
    }

    async fn do_transform(config: LogToMetricConfig, event: Event) -> Option<Event> {
        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;
            tx.send(event).await.unwrap();
            let result = tokio::time::timeout(Duration::from_secs(5), out.recv())
                .await
                .unwrap_or(None);
            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
            result
        })
        .await
    }

    async fn do_transform_multiple_events(
        config: LogToMetricConfig,
        event: Event,
        count: usize,
    ) -> Vec<Event> {
        assert_transform_compliance(async move {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;
            tx.send(event).await.unwrap();

            let mut results = vec![];
            for _ in 0..count {
                let result = tokio::time::timeout(Duration::from_secs(5), out.recv())
                    .await
                    .unwrap_or(None);
                if let Some(event) = result {
                    results.push(event);
                }
            }

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
            results
        })
        .await
    }

    #[tokio::test]
    async fn count_http_status_codes() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            "#,
        );

        let event = create_event("status", "42");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "status",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                metadata,
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn count_http_requests_with_tags() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "message"
            name = "http_requests_total"
            namespace = "app"
            tags = {method = "{{method}}", code = "{{code}}", missing_tag = "{{unknown}}", host = "localhost"}
            "#,
        );

        let mut event = create_event("message", "i am log");
        event.as_mut_log().insert("method", "post");
        event.as_mut_log().insert("code", "200");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "http_requests_total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                metadata,
            )
            .with_namespace(Some("app"))
            .with_tags(Some(metric_tags!(
                "method" => "post",
                "code" => "200",
                "host" => "localhost",
            )))
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn multi_value_tags_yaml() {
        // Have to use YAML to represent bare tags
        let config = parse_yaml_config(
            r#"
            metrics:
            - field: "message"
              type: "counter"
              tags:
                tag:
                - "one"
                - null
                - "two"
            "#,
        );

        let event = create_event("message", "I am log");
        let metric = do_transform(config, event).await.unwrap().into_metric();
        let tags = metric.tags().expect("Metric should have tags");

        assert_eq!(tags.iter_single().collect::<Vec<_>>(), vec![("tag", "two")]);

        assert_eq!(tags.iter_all().count(), 3);
        for (name, value) in tags.iter_all() {
            assert_eq!(name, "tag");
            assert!(value.is_none() || value == Some("one") || value == Some("two"));
        }
    }

    #[tokio::test]
    async fn multi_value_tags_toml() {
        let config = parse_config(
            r#"
            [[metrics]]
            field = "message"
            type = "counter"
            [metrics.tags]
            tag = ["one", "two"]
            "#,
        );

        let event = create_event("message", "I am log");
        let metric = do_transform(config, event).await.unwrap().into_metric();
        let tags = metric.tags().expect("Metric should have tags");

        assert_eq!(tags.iter_single().collect::<Vec<_>>(), vec![("tag", "two")]);

        assert_eq!(tags.iter_all().count(), 2);
        for (name, value) in tags.iter_all() {
            assert_eq!(name, "tag");
            assert!(value == Some("one") || value == Some("two"));
        }
    }

    #[tokio::test]
    async fn count_exceptions() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            "#,
        );

        let event = create_event("backtrace", "message");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "exception_total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                metadata
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn count_exceptions_no_match() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            "#,
        );

        let event = create_event("success", "42");
        assert_eq!(do_transform(config, event).await, None);
    }

    #[tokio::test]
    async fn sum_order_amounts() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "amount"
            name = "amount_total"
            increment_by_value = true
            "#,
        );

        let event = create_event("amount", "33.99");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "amount_total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 33.99 },
                metadata,
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn count_absolute() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "amount"
            name = "amount_total"
            increment_by_value = true
            kind = "absolute"
            "#,
        );

        let event = create_event("amount", "33.99");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "amount_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 33.99 },
                metadata,
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn memory_usage_gauge() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "gauge"
            field = "memory_rss"
            name = "memory_rss_bytes"
            "#,
        );

        let event = create_event("memory_rss", "123");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "memory_rss_bytes",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 123.0 },
                metadata,
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn parse_failure() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            increment_by_value = true
            "#,
        );

        let event = create_event("status", "not a number");
        assert_eq!(do_transform(config, event).await, None);
    }

    #[tokio::test]
    async fn missing_field() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            "#,
        );

        let event = create_event("not foo", "not a number");
        assert_eq!(do_transform(config, event).await, None);
    }

    #[tokio::test]
    async fn null_field() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            "#,
        );

        let event = create_event("status", Value::Null);
        assert_eq!(do_transform(config, event).await, None);
    }

    #[tokio::test]
    async fn multiple_metrics() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"

            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            "#,
        );

        let mut event = Event::Log(LogEvent::from("i am a log"));
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts(),
        );
        event.as_mut_log().insert("status", "42");
        event.as_mut_log().insert("backtrace", "message");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let output = do_transform_multiple_events(config, event, 2).await;

        assert_eq!(2, output.len());
        assert_eq!(
            output[0].clone().into_metric(),
            Metric::new_with_metadata(
                "status",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                metadata.clone(),
            )
            .with_timestamp(Some(ts()))
        );
        assert_eq!(
            output[1].clone().into_metric(),
            Metric::new_with_metadata(
                "exception_total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                metadata,
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn multiple_metrics_with_multiple_templates() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "set"
            field = "status"
            name = "{{host}}_{{worker}}_status_set"

            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "{{service}}_exception_total"
            namespace = "{{host}}"
            "#,
        );

        let mut event = Event::Log(LogEvent::from("i am a log"));
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts(),
        );
        event.as_mut_log().insert("status", "42");
        event.as_mut_log().insert("backtrace", "message");
        event.as_mut_log().insert("host", "local");
        event.as_mut_log().insert("worker", "abc");
        event.as_mut_log().insert("service", "xyz");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));

        let output = do_transform_multiple_events(config, event, 2).await;

        assert_eq!(2, output.len());
        assert_eq!(
            output[0].as_metric(),
            &Metric::new_with_metadata(
                "local_abc_status_set",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["42".into()].into_iter().collect()
                },
                metadata.clone(),
            )
            .with_timestamp(Some(ts()))
        );
        assert_eq!(
            output[1].as_metric(),
            &Metric::new_with_metadata(
                "xyz_exception_total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                metadata,
            )
            .with_namespace(Some("local"))
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn user_ip_set() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "set"
            field = "user_ip"
            name = "unique_user_ip"
            "#,
        );

        let event = create_event("user_ip", "1.2.3.4");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "unique_user_ip",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["1.2.3.4".into()].into_iter().collect()
                },
                metadata,
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn response_time_histogram() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "histogram"
            field = "response_time"
            "#,
        );

        let event = create_event("response_time", "2.5");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "response_time",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_core::samples![2.5 => 1],
                    statistic: StatisticKind::Histogram
                },
                metadata
            )
            .with_timestamp(Some(ts()))
        );
    }

    #[tokio::test]
    async fn response_time_summary() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "summary"
            field = "response_time"
            "#,
        );

        let event = create_event("response_time", "2.5");
        let mut metadata = event.metadata().clone();
        metadata.set_source_id(Arc::new(OutputId::from("in")));
        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "response_time",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_core::samples![2.5 => 1],
                    statistic: StatisticKind::Summary
                },
                metadata
            )
            .with_timestamp(Some(ts()))
        );
    }
}
