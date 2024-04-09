use std::sync::Arc;
use std::{collections::HashMap, num::ParseFloatError};

use chrono::Utc;
use indexmap::IndexMap;
use vector_lib::configurable::configurable_component;
use vector_lib::event::LogEvent;
use vector_lib::{
    config::LogNamespace,
    event::DatadogMetricOriginMetadata,
    event::{
        metric::Sample,
        metric::{Bucket, Quantile},
    },
};
use vrl::path::{parse_target_path, PathParseError};
use vrl::{event_path, path};

use crate::config::schema::Definition;
use crate::transforms::log_to_metric::TransformError::PathNotFound;
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
        LogToMetricFieldNullError, LogToMetricParseFloatError,
        MetricMetadataInvalidFieldValueError, MetricMetadataMetricDetailsNotFoundError,
        MetricMetadataParseError, ParserMissingFieldError, DROP_EVENT,
    },
    schema,
    template::{Template, TemplateRenderingError},
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

const ORIGIN_SERVICE_VALUE: u32 = 3;

/// Configuration for the `log_to_metric` transform.
#[configurable_component(transform("log_to_metric", "Convert log events to metric events."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LogToMetricConfig {
    /// A list of metrics to generate.
    pub metrics: Vec<MetricConfig>,
    /// Setting this flag changes the behavior of this transformation.<br />
    /// <p>Notably the `metrics` field will be ignored.</p>
    /// <p>All incoming events will be processed and if possible they will be converted to log events.
    /// Otherwise, only items specified in the 'metrics' field will be processed.</p>
    /// <pre class="chroma"><code class="language-toml" data-lang="toml">use serde_json::json;
    /// let json_event = json!({
    ///     "counter": {
    ///         "value": 10.0
    ///     },
    ///     "kind": "incremental",
    ///     "name": "test.transform.counter",
    ///     "tags": {
    ///         "env": "test_env",
    ///         "host": "localhost"
    ///     }
    /// });
    /// </code></pre>
    ///
    /// This is an example JSON representation of a counter with the following properties:
    ///
    /// - `counter`: An object with a single property `value` representing the counter value, in this case, `10.0`).
    /// - `kind`: A string indicating the kind of counter, in this case, "incremental".
    /// - `name`: A string representing the name of the counter, here set to "test.transform.counter".
    /// - `tags`: An object containing additional tags such as "env" and "host".
    ///
    /// Objects that can be processed include counter, histogram, gauge, set and summary.
    pub all_metrics: Option<bool>,
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
            all_metrics: Some(true),
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
        _: vector_lib::enrichment::TableRegistry,
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

/// Kinds of TranformError for Parsing
#[configurable_component]
#[derive(Clone, Debug)]
pub enum TransformParseErrorKind {
    ///  Error when Parsing a Float
    FloatError,
    ///  Error when Parsing an Int
    IntError,
    /// Errors when Parsing Arrays
    ArrayError,
}

impl std::fmt::Display for TransformParseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

enum TransformError {
    PathNotFound {
        path: String,
    },
    PathNull {
        path: String,
    },
    MetricDetailsNotFound,
    MetricValueError {
        path: String,
        path_value: String,
    },
    ParseError {
        path: String,
        kind: TransformParseErrorKind,
    },
    ParseFloatError {
        path: String,
        error: ParseFloatError,
    },
    TemplateRenderingError(TemplateRenderingError),
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

fn to_metric_with_config(config: &MetricConfig, event: &Event) -> Result<Metric, TransformError> {
    let log = event.as_log();

    let timestamp = log
        .get_timestamp()
        .and_then(Value::as_timestamp)
        .cloned()
        .or_else(|| Some(Utc::now()));

    // Assign the OriginService for the new metric
    let metadata = event
        .metadata()
        .clone()
        .with_schema_definition(&Arc::new(Definition::any()))
        .with_origin_metadata(DatadogMetricOriginMetadata::new(
            None,
            None,
            Some(ORIGIN_SERVICE_VALUE),
        ));

    let field = parse_target_path(config.field()).map_err(|_e| PathNotFound {
        path: config.field().to_string(),
    })?;

    let value = match log.get(&field) {
        None => Err(TransformError::PathNotFound {
            path: field.to_string(),
        }),
        Some(Value::Null) => Err(TransformError::PathNull {
            path: field.to_string(),
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
                        path: config.field.get_ref().to_owned(),
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
                    path: field.to_string(),
                    error,
                }
            })?;

            (
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![value => 1],
                    statistic: StatisticKind::Histogram,
                },
            )
        }
        MetricTypeConfig::Summary => {
            let value = value.to_string_lossy().parse().map_err(|error| {
                TransformError::ParseFloatError {
                    path: field.to_string(),
                    error,
                }
            })?;

            (
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![value => 1],
                    statistic: StatisticKind::Summary,
                },
            )
        }
        MetricTypeConfig::Gauge => {
            let value = value.to_string_lossy().parse().map_err(|error| {
                TransformError::ParseFloatError {
                    path: field.to_string(),
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

fn bytes_to_str(value: &Value) -> Option<String> {
    match value {
        Value::Bytes(bytes) => std::str::from_utf8(bytes).ok().map(|s| s.to_string()),
        _ => None,
    }
}

fn try_get_string_from_log(log: &LogEvent, path: &str) -> Result<Option<String>, TransformError> {
    // TODO: update returned errors after `TransformError` is refactored.
    let maybe_value = log.parse_path_and_get_value(path).map_err(|e| match e {
        PathParseError::InvalidPathSyntax { path } => PathNotFound {
            path: path.to_string(),
        },
    })?;
    match maybe_value {
        None => Err(PathNotFound {
            path: path.to_string(),
        }),
        Some(v) => Ok(bytes_to_str(v)),
    }
}

fn get_counter_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let counter_value = log
        .get(event_path!("counter", "value"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "counter.value".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseError {
            path: "counter.value".to_string(),
            kind: TransformParseErrorKind::FloatError,
        })?;

    Ok(MetricValue::Counter {
        value: *counter_value,
    })
}

fn get_gauge_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let gauge_value = log
        .get(event_path!("gauge", "value"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "gauge.value".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseError {
            path: "gauge.value".to_string(),
            kind: TransformParseErrorKind::FloatError,
        })?;
    Ok(MetricValue::Gauge {
        value: *gauge_value,
    })
}

fn get_set_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let set_values = log
        .get(event_path!("set", "values"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "set.values".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseError {
            path: "set.values".to_string(),
            kind: TransformParseErrorKind::ArrayError,
        })?;

    let mut values: Vec<String> = Vec::new();
    for e_value in set_values {
        let value = e_value
            .as_bytes()
            .ok_or_else(|| TransformError::ParseError {
                path: "set.values".to_string(),
                kind: TransformParseErrorKind::ArrayError,
            })?;
        values.push(String::from_utf8_lossy(value).to_string());
    }

    Ok(MetricValue::Set {
        values: values.into_iter().collect(),
    })
}

fn get_distribution_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let event_samples = log
        .get(event_path!("distribution", "samples"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "distribution.samples".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseError {
            path: "distribution.samples".to_string(),
            kind: TransformParseErrorKind::ArrayError,
        })?;

    let mut samples: Vec<Sample> = Vec::new();
    for e_sample in event_samples {
        let value = e_sample
            .get(path!("value"))
            .ok_or_else(|| TransformError::PathNotFound {
                path: "value".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseError {
                path: "value".to_string(),
                kind: TransformParseErrorKind::FloatError,
            })?;

        let rate = e_sample
            .get(path!("rate"))
            .ok_or_else(|| TransformError::PathNotFound {
                path: "rate".to_string(),
            })?
            .as_integer()
            .ok_or_else(|| TransformError::ParseError {
                path: "rate".to_string(),
                kind: TransformParseErrorKind::IntError,
            })?;

        samples.push(Sample {
            value: *value,
            rate: rate as u32,
        });
    }

    let statistic_str = match try_get_string_from_log(log, "distribution.statistic")? {
        Some(n) => n,
        None => {
            return Err(TransformError::PathNotFound {
                path: "distribution.statistic".to_string(),
            })
        }
    };
    let statistic_kind = match statistic_str.as_str() {
        "histogram" => Ok(StatisticKind::Histogram),
        "summary" => Ok(StatisticKind::Summary),
        _ => Err(TransformError::MetricValueError {
            path: "distribution.statistic".to_string(),
            path_value: statistic_str.to_string(),
        }),
    }?;

    Ok(MetricValue::Distribution {
        samples,
        statistic: statistic_kind,
    })
}

fn get_histogram_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let event_buckets = log
        .get(event_path!("histogram", "buckets"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "histogram.buckets".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseError {
            path: "histogram.buckets".to_string(),
            kind: TransformParseErrorKind::ArrayError,
        })?;

    let mut buckets: Vec<Bucket> = Vec::new();
    for e_bucket in event_buckets {
        let upper_limit = e_bucket
            .get(path!("upper_limit"))
            .ok_or_else(|| TransformError::PathNotFound {
                path: "histogram.buckets.upper_limit".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseError {
                path: "histogram.buckets.upper_limit".to_string(),
                kind: TransformParseErrorKind::FloatError,
            })?;

        let count = e_bucket
            .get(path!("count"))
            .ok_or_else(|| TransformError::PathNotFound {
                path: "histogram.buckets.count".to_string(),
            })?
            .as_integer()
            .ok_or_else(|| TransformError::ParseError {
                path: "histogram.buckets.count".to_string(),
                kind: TransformParseErrorKind::IntError,
            })?;

        buckets.push(Bucket {
            upper_limit: *upper_limit,
            count: count as u64,
        });
    }

    let count = log
        .get(event_path!("histogram", "count"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "histogram.count".to_string(),
        })?
        .as_integer()
        .ok_or_else(|| TransformError::ParseError {
            path: "histogram.count".to_string(),
            kind: TransformParseErrorKind::IntError,
        })?;

    let sum = log
        .get(event_path!("histogram", "sum"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "histogram.sum".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseError {
            path: "histogram.sum".to_string(),
            kind: TransformParseErrorKind::FloatError,
        })?;

    Ok(MetricValue::AggregatedHistogram {
        buckets,
        count: count as u64,
        sum: *sum,
    })
}

fn get_summary_value(log: &LogEvent) -> Result<MetricValue, TransformError> {
    let event_quantiles = log
        .get(event_path!("summary", "quantiles"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "summary.quantiles".to_string(),
        })?
        .as_array()
        .ok_or_else(|| TransformError::ParseError {
            path: "summary.quantiles".to_string(),
            kind: TransformParseErrorKind::ArrayError,
        })?;

    let mut quantiles: Vec<Quantile> = Vec::new();
    for e_quantile in event_quantiles {
        let quantile = e_quantile
            .get(path!("quantile"))
            .ok_or_else(|| TransformError::PathNotFound {
                path: "summary.quantiles.quantile".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseError {
                path: "summary.quantiles.quantile".to_string(),
                kind: TransformParseErrorKind::FloatError,
            })?;

        let value = e_quantile
            .get(path!("value"))
            .ok_or_else(|| TransformError::PathNotFound {
                path: "summary.quantiles.value".to_string(),
            })?
            .as_float()
            .ok_or_else(|| TransformError::ParseError {
                path: "summary.quantiles.value".to_string(),
                kind: TransformParseErrorKind::FloatError,
            })?;

        quantiles.push(Quantile {
            quantile: *quantile,
            value: *value,
        })
    }

    let count = log
        .get(event_path!("summary", "count"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "summary.count".to_string(),
        })?
        .as_integer()
        .ok_or_else(|| TransformError::ParseError {
            path: "summary.count".to_string(),
            kind: TransformParseErrorKind::IntError,
        })?;

    let sum = log
        .get(event_path!("summary", "sum"))
        .ok_or_else(|| TransformError::PathNotFound {
            path: "summary.sum".to_string(),
        })?
        .as_float()
        .ok_or_else(|| TransformError::ParseError {
            path: "summary.sum".to_string(),
            kind: TransformParseErrorKind::FloatError,
        })?;

    Ok(MetricValue::AggregatedSummary {
        quantiles,
        count: count as u64,
        sum: *sum,
    })
}

fn to_metrics(event: &Event) -> Result<Metric, TransformError> {
    let log = event.as_log();
    let timestamp = log
        .get_timestamp()
        .and_then(Value::as_timestamp)
        .cloned()
        .or_else(|| Some(Utc::now()));

    let name = match try_get_string_from_log(log, "name")? {
        Some(n) => n,
        None => {
            return Err(TransformError::PathNotFound {
                path: "name".to_string(),
            })
        }
    };

    let tags = &mut MetricTags::default();

    if let Some(els) = log.get(event_path!("tags")) {
        if let Some(el) = els.as_object() {
            for (key, value) in el {
                tags.insert(key.to_string(), bytes_to_str(value));
            }
        }
    }
    let tags_result = Some(tags.clone());

    let kind_str = match try_get_string_from_log(log, "kind")? {
        Some(n) => n,
        None => {
            return Err(TransformError::PathNotFound {
                path: "kind".to_string(),
            })
        }
    };

    let kind = match kind_str.as_str() {
        "absolute" => Ok(MetricKind::Absolute),
        "incremental" => Ok(MetricKind::Incremental),
        value => Err(TransformError::MetricValueError {
            path: "kind".to_string(),
            path_value: value.to_string(),
        }),
    }?;

    let mut value: Option<MetricValue> = None;
    if let Some(root_event) = log.as_map() {
        for key in root_event.keys() {
            value = match key.as_str() {
                "gauge" => Some(get_gauge_value(log)?),
                "distribution" => Some(get_distribution_value(log)?),
                "histogram" => Some(get_histogram_value(log)?),
                "summary" => Some(get_summary_value(log)?),
                "counter" => Some(get_counter_value(log)?),
                "set" => Some(get_set_value(log)?),
                _ => None,
            };

            if value.is_some() {
                break;
            }
        }
    }

    let value = value.ok_or(TransformError::MetricDetailsNotFound)?;

    Ok(
        Metric::new_with_metadata(name, kind, value, log.metadata().clone())
            .with_namespace(try_get_string_from_log(log, "namespace")?)
            .with_tags(tags_result)
            .with_timestamp(timestamp),
    )
}

impl FunctionTransform for LogToMetric {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        // Metrics are "all or none" for a specific log. If a single fails, none are produced.
        let mut buffer = Vec::with_capacity(self.config.metrics.len());
        if self
            .config
            .all_metrics
            .is_some_and(|all_metrics| all_metrics)
        {
            match to_metrics(&event) {
                Ok(metric) => {
                    output.push(Event::Metric(metric));
                }
                Err(err) => {
                    match err {
                        TransformError::MetricValueError { path, path_value } => {
                            emit!(MetricMetadataInvalidFieldValueError {
                                field: path.as_ref(),
                                field_value: path_value.as_ref()
                            })
                        }
                        TransformError::PathNotFound { path } => {
                            emit!(ParserMissingFieldError::<DROP_EVENT> {
                                field: path.as_ref()
                            })
                        }
                        TransformError::ParseError { path, kind } => {
                            emit!(MetricMetadataParseError {
                                field: path.as_ref(),
                                kind: &kind.to_string(),
                            })
                        }
                        TransformError::MetricDetailsNotFound {} => {
                            emit!(MetricMetadataMetricDetailsNotFoundError {})
                        }
                        _ => {}
                    };
                }
            }
        } else {
            for config in self.config.metrics.iter() {
                match to_metric_with_config(config, &event) {
                    Ok(metric) => {
                        buffer.push(Event::Metric(metric));
                    }
                    Err(err) => {
                        match err {
                            TransformError::PathNull { path } => {
                                emit!(LogToMetricFieldNullError {
                                    field: path.as_ref()
                                })
                            }
                            TransformError::PathNotFound { path } => {
                                emit!(ParserMissingFieldError::<DROP_EVENT> {
                                    field: path.as_ref()
                                })
                            }
                            TransformError::ParseFloatError { path, error } => {
                                emit!(LogToMetricParseFloatError {
                                    field: path.as_ref(),
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
                            _ => {}
                        };
                        // early return to prevent the partial buffer from being sent
                        return;
                    }
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
    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::config::ComponentKey;
    use vector_lib::event::EventMetadata;
    use vector_lib::metric_tags;

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
        log.as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts());
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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));
        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));
        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));
        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));
        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));
        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));

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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));

        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));

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
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts());
        event.as_mut_log().insert("status", "42");
        event.as_mut_log().insert("backtrace", "message");
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

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
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts());
        event.as_mut_log().insert("status", "42");
        event.as_mut_log().insert("backtrace", "message");
        event.as_mut_log().insert("host", "local");
        event.as_mut_log().insert("worker", "abc");
        event.as_mut_log().insert("service", "xyz");
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));
        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "response_time",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![2.5 => 1],
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
        let mut metadata =
            event
                .metadata()
                .clone()
                .with_origin_metadata(DatadogMetricOriginMetadata::new(
                    None,
                    None,
                    Some(ORIGIN_SERVICE_VALUE),
                ));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        metadata.set_schema_definition(&Arc::new(Definition::any()));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));

        let metric = do_transform(config, event).await.unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::new_with_metadata(
                "response_time",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_lib::samples![2.5 => 1],
                    statistic: StatisticKind::Summary
                },
                metadata
            )
            .with_timestamp(Some(ts()))
        );
    }

    //  Metric Metadata Tests
    fn create_log_event(json_str: &str) -> Event {
        let mut log_value: Value =
            serde_json::from_str(json_str).expect("JSON was not well-formatted");
        log_value.insert("timestamp", ts());
        log_value.insert("namespace", "test_namespace");

        let mut metadata = EventMetadata::default();
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));

        Event::Log(LogEvent::from_parts(log_value, metadata.clone()))
    }

    #[tokio::test]
    async fn transform_gauge() {
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

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
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
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
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

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
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
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
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

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
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
            Metric::new_with_metadata(
                "test.transform.distribution_histogram",
                MetricKind::Absolute,
                MetricValue::Distribution {
                    samples: vec![
                        Sample {
                            value: 1.0,
                            rate: 1
                        },
                        Sample {
                            value: 2.0,
                            rate: 2
                        },
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
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

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
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
            Metric::new_with_metadata(
                "test.transform.distribution_summary",
                MetricKind::Absolute,
                MetricValue::Distribution {
                    samples: vec![
                        Sample {
                            value: 1.0,
                            rate: 1
                        },
                        Sample {
                            value: 2.0,
                            rate: 2
                        },
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
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

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
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
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
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

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
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
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

    #[tokio::test]
    async fn transform_set() {
        let config = parse_yaml_config(
            r#"
            metrics: []
            all_metrics: true
            "#,
        );

        let json_str = r#"{
          "set": {
            "values": ["990.0", "1234"]
          },
          "kind": "incremental",
          "name": "test.transform.set",
          "tags": {
            "env": "test_env",
            "host": "localhost"
          }
        }"#;
        let log = create_log_event(json_str);
        let metric = do_transform(config, log.clone()).await.unwrap();
        assert_eq!(
            *metric.as_metric(),
            Metric::new_with_metadata(
                "test.transform.set",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["990.0".into(), "1234".into()].into_iter().collect()
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
}
