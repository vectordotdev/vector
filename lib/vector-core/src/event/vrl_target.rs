use std::{
    borrow::Cow,
    collections::BTreeMap,
    convert::TryFrom,
    marker::PhantomData,
    num::{NonZero, TryFromIntError},
};

use lookup::{OwnedTargetPath, OwnedValuePath, PathPrefix, lookup_v2::OwnedSegment};
use snafu::Snafu;
use vrl::{
    compiler::{ProgramInfo, SecretTarget, Target, value::VrlValueConvert},
    prelude::Collection,
    value::{Kind, ObjectMap, Value},
};

use super::{
    Event, EventMetadata, LogEvent, Metric, MetricKind, MetricValue, TraceEvent,
    metric::{
        Bucket, MetricSketch, MetricTags, Quantile, Sample, StatisticKind, TagValue, TagValueSet,
    },
};
use crate::{
    config::{LogNamespace, log_schema},
    schema::Definition,
};

const VALID_METRIC_PATHS_SET: &str =
    ".name, .namespace, .interval_ms, .timestamp, .kind, .tags, .type";

const VALID_METRIC_PATHS_GET: &str = ".name, .namespace, .interval_ms, .timestamp, .kind, .tags, .type, \
    .counter, .gauge, .set, .distribution, .aggregated_histogram, .aggregated_summary, .sketch";

/// The fields that hold a metric's value, one per metric type.
///
/// These match the native JSON encoding of metrics, which is what `vector tap` shows. Only the
/// field for the metric's current type is present; the others read as `null`.
const METRIC_VALUE_FIELDS: [&str; 7] = [
    "counter",
    "gauge",
    "set",
    "distribution",
    "aggregated_histogram",
    "aggregated_summary",
    "sketch",
];

/// Metrics aren't interested in paths that have a length longer than 3.
///
/// The longest path is 2, and we need to check that a third segment doesn't exist as we don't want
/// fields such as `.tags.host.thing`.
const MAX_METRIC_PATH_DEPTH: usize = 3;

/// How metric tags are exposed to and accepted from VRL or Lua.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum MetricTagMode {
    /// Tags are exposed as single strings (last value wins for multi-value
    /// tags); writes always produce single-value tags.
    #[default]
    Single,
    /// Tags are always exposed as arrays; writes always produce multi-value
    /// tags regardless of whether the assigned value is scalar or array.
    Full,
    /// Tags are exposed using their underlying shape: single-value tags as
    /// strings, multi-value tags as arrays. Writes: scalar values produce
    /// single-value tags; arrays of length >= 2 produce multi-value tags.
    ///
    /// A length-1 array is normalised to a single-value tag by the metric
    /// storage layer (`TagValueSet::Set` is never reduced below 2 elements),
    /// so an assignment like `.tags.region = ["us-east-1"]` round-trips as
    /// a scalar on the next read. Use `Full` to force array shape regardless
    /// of length.
    Auto,
}

/// An adapter to turn `Event`s into `vrl_lib::Target`s.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum VrlTarget {
    // `LogEvent` is essentially just a destructured `event::LogEvent`, but without the semantics
    // that `fields` must always be a `Map` variant.
    LogEvent(Value, EventMetadata),
    Metric {
        metric: Metric,
        value: Value,
        tag_mode: MetricTagMode,
    },
    Trace(Value, EventMetadata),
}

pub enum TargetEvents {
    One(Event),
    Logs(TargetIter<LogEvent>),
    Traces(TargetIter<TraceEvent>),
}

pub struct TargetIter<T> {
    iter: std::vec::IntoIter<Value>,
    metadata: EventMetadata,
    _marker: PhantomData<T>,
    log_namespace: LogNamespace,
}

fn create_log_event(value: Value, metadata: EventMetadata) -> LogEvent {
    let mut log = LogEvent::new_with_metadata(metadata);
    log.maybe_insert(log_schema().message_key_target_path(), value);
    log
}

impl Iterator for TargetIter<LogEvent> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|v| {
            match self.log_namespace {
                LogNamespace::Legacy => match v {
                    value @ Value::Object(_) => LogEvent::from_parts(value, self.metadata.clone()),
                    value => create_log_event(value, self.metadata.clone()),
                },
                LogNamespace::Vector => LogEvent::from_parts(v, self.metadata.clone()),
            }
            .into()
        })
    }
}

impl Iterator for TargetIter<TraceEvent> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|v| {
            match v {
                value @ Value::Object(_) => {
                    TraceEvent::from(LogEvent::from_parts(value, self.metadata.clone()))
                }
                value => TraceEvent::from(create_log_event(value, self.metadata.clone())),
            }
            .into()
        })
    }
}

impl VrlTarget {
    pub fn new(event: Event, info: &ProgramInfo, tag_mode: MetricTagMode) -> Self {
        match event {
            Event::Log(event) => {
                let (value, metadata) = event.into_parts();
                VrlTarget::LogEvent(value, metadata)
            }
            Event::Metric(metric) => {
                // We pre-generate [`Value`] types for the metric fields accessed in
                // the event. This allows us to then return references to those
                // values, even if the field is accessed more than once.
                let value = precompute_metric_value(&metric, info, tag_mode);

                VrlTarget::Metric {
                    metric,
                    value,
                    tag_mode,
                }
            }
            Event::Trace(event) => {
                let (fields, metadata) = event.into_parts();
                VrlTarget::Trace(Value::Object(fields), metadata)
            }
        }
    }

    /// Modifies a schema in the same way that the `into_events` function modifies the event
    pub fn modify_schema_definition_for_into_events(input: Definition) -> Definition {
        let log_namespaces = input.log_namespaces().clone();

        // both namespaces merge arrays, but only `Legacy` moves field definitions into a "message" field.
        let merged_arrays = merge_array_definitions(input);
        Definition::combine_log_namespaces(
            &log_namespaces,
            move_field_definitions_into_message(merged_arrays.clone()),
            merged_arrays,
        )
    }

    /// Turn the target back into events.
    ///
    /// This returns an iterator of events as one event can be turned into multiple by assigning an
    /// array to `.` in VRL.
    pub fn into_events(self, log_namespace: LogNamespace) -> TargetEvents {
        match self {
            VrlTarget::LogEvent(value, metadata) => match value {
                value @ Value::Object(_) => {
                    TargetEvents::One(LogEvent::from_parts(value, metadata).into())
                }

                Value::Array(values) => TargetEvents::Logs(TargetIter {
                    iter: values.into_iter(),
                    metadata,
                    _marker: PhantomData,
                    log_namespace,
                }),

                v => match log_namespace {
                    LogNamespace::Vector => {
                        TargetEvents::One(LogEvent::from_parts(v, metadata).into())
                    }
                    LogNamespace::Legacy => TargetEvents::One(create_log_event(v, metadata).into()),
                },
            },
            VrlTarget::Trace(value, mut metadata) => match value {
                value @ Value::Object(_) => {
                    let log = LogEvent::from_parts(value, metadata);
                    TargetEvents::One(TraceEvent::from(log).into())
                }

                Value::Array(values) => TargetEvents::Traces(TargetIter {
                    iter: values.into_iter(),
                    metadata,
                    _marker: PhantomData,
                    log_namespace,
                }),

                v => {
                    metadata.clear_trace_layout();
                    TargetEvents::One(create_log_event(v, metadata).into())
                }
            },
            VrlTarget::Metric { metric, .. } => TargetEvents::One(Event::Metric(metric)),
        }
    }

    fn metadata(&self) -> &EventMetadata {
        match self {
            VrlTarget::LogEvent(_, metadata) | VrlTarget::Trace(_, metadata) => metadata,
            VrlTarget::Metric { metric, .. } => metric.metadata(),
        }
    }

    fn metadata_mut(&mut self) -> &mut EventMetadata {
        match self {
            VrlTarget::LogEvent(_, metadata) | VrlTarget::Trace(_, metadata) => metadata,
            VrlTarget::Metric { metric, .. } => metric.metadata_mut(),
        }
    }
}

/// If the VRL returns a value that is not an array (see [`merge_array_definitions`]),
/// or an object, that data is moved into the `message` field.
fn move_field_definitions_into_message(mut definition: Definition) -> Definition {
    let mut message = definition.event_kind().clone();
    message.remove_object();
    message.remove_array();

    if !message.is_never()
        && let Some(message_key) = log_schema().message_key()
    {
        // We need to add the given message type to a field called `message`
        // in the event.
        let message = Kind::object(Collection::from(BTreeMap::from([(
            message_key.to_string().into(),
            message,
        )])));

        definition.event_kind_mut().remove_bytes();
        definition.event_kind_mut().remove_integer();
        definition.event_kind_mut().remove_float();
        definition.event_kind_mut().remove_boolean();
        definition.event_kind_mut().remove_timestamp();
        definition.event_kind_mut().remove_regex();
        definition.event_kind_mut().remove_null();

        *definition.event_kind_mut() = definition.event_kind().union(message);
    }

    definition
}

/// If the transform returns an array, the elements of this array will be separated
/// out into it's individual elements and passed downstream.
///
/// The potential types that the transform can output are any of the arrays
/// elements or any non-array elements that are within the definition. All these
/// definitions need to be merged together.
fn merge_array_definitions(mut definition: Definition) -> Definition {
    if let Some(array) = definition.event_kind().as_array() {
        let array_kinds = array.reduced_kind();

        let kind = definition.event_kind_mut();
        kind.remove_array();
        *kind = kind.union(array_kinds);
    }

    definition
}

fn set_metric_tag_values(
    name: String,
    value: &Value,
    metric: &mut Metric,
    tag_mode: MetricTagMode,
) {
    // `Auto` dispatches by the *shape* of the assigned value: arrays go through
    // the multi-value path, scalars go through the single-value path. `Full`
    // and `Single` are unchanged.
    let multi_value = match tag_mode {
        MetricTagMode::Single => false,
        MetricTagMode::Full => true,
        MetricTagMode::Auto => matches!(value, Value::Array(_)),
    };

    if multi_value {
        let values = if let Value::Array(values) = value {
            values.as_slice()
        } else {
            std::slice::from_ref(value)
        };

        let tag_values = values
            .iter()
            .filter_map(|value| match value {
                Value::Bytes(bytes) => {
                    Some(TagValue::Value(String::from_utf8_lossy(bytes).to_string()))
                }
                Value::String(string) => Some(TagValue::Value(string.to_string())),
                Value::Null => Some(TagValue::Bare),
                _ => None,
            })
            .collect::<Vec<_>>();

        metric.set_multi_value_tag(name, tag_values);
    } else {
        // set a single tag value
        if let Ok(tag_value) = value.try_bytes_utf8_lossy().map(Cow::into_owned) {
            metric.replace_tag(name, tag_value);
        } else if value.is_null() {
            metric.set_multi_value_tag(name, vec![TagValue::Bare]);
        }
    }
}

/// Converts a counter to a gauge or a gauge to a counter, keeping its value and kind.
///
/// Other metric types carry values that have no meaningful scalar equivalent, so they are
/// rejected both as the current type and as the target type.
fn set_metric_type(metric: &mut Metric, value: &Value) -> Result<(), String> {
    let target_type = value.clone().try_bytes().map_err(|e| e.to_string())?;
    let current = match metric.value() {
        MetricValue::Counter { value } | MetricValue::Gauge { value } => *value,
        other => {
            return Err(format!(
                "cannot change the type of a {} metric, only `counter` and `gauge` metrics can be converted",
                other.as_name()
            ));
        }
    };
    *metric.value_mut() = match target_type.as_ref() {
        b"counter" => MetricValue::Counter { value: current },
        b"gauge" => MetricValue::Gauge { value: current },
        other => {
            return Err(format!(
                "invalid metric type {}, metric type must be `counter` or `gauge`",
                String::from_utf8_lossy(other)
            ));
        }
    };
    Ok(())
}

impl Target for VrlTarget {
    fn target_insert(&mut self, target_path: &OwnedTargetPath, value: Value) -> Result<(), String> {
        let path = &target_path.path;
        match target_path.prefix {
            PathPrefix::Event => match self {
                VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => {
                    log.insert(path, value);
                    Ok(())
                }
                VrlTarget::Metric {
                    metric,
                    value: metric_value,
                    tag_mode,
                } => {
                    if path.is_root() {
                        return Err(MetricPathError::SetPathError.to_string());
                    }

                    if let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH) {
                        match paths.as_slice() {
                            ["tags"] => {
                                let value =
                                    value.clone().try_object().map_err(|e| e.to_string())?;

                                metric.remove_tags();
                                for (field, value) in &value {
                                    set_metric_tag_values(
                                        field.as_str().into(),
                                        value,
                                        metric,
                                        *tag_mode,
                                    );
                                }
                            }
                            ["tags", field] => {
                                set_metric_tag_values(
                                    (*field).to_owned(),
                                    &value,
                                    metric,
                                    *tag_mode,
                                );
                            }
                            ["name"] => {
                                let value = value.clone().try_bytes().map_err(|e| e.to_string())?;
                                metric.series.name.name =
                                    String::from_utf8_lossy(&value).into_owned();
                            }
                            ["namespace"] => {
                                let value = value.clone().try_bytes().map_err(|e| e.to_string())?;
                                metric.series.name.namespace =
                                    Some(String::from_utf8_lossy(&value).into_owned());
                            }
                            ["interval_ms"] => {
                                let value: i64 =
                                    value.clone().try_into_i64().map_err(|e| e.to_string())?;
                                let value: u32 = value
                                    .try_into()
                                    .map_err(|e: TryFromIntError| e.to_string())?;
                                let value = NonZero::try_from(value).map_err(|e| e.to_string())?;
                                metric.data.time.interval_ms = Some(value);
                            }
                            ["timestamp"] => {
                                let value =
                                    value.clone().try_timestamp().map_err(|e| e.to_string())?;
                                metric.data.time.timestamp = Some(value);
                            }
                            ["kind"] => {
                                metric.data.kind = MetricKind::try_from(value.clone())?;
                            }
                            ["type"] => {
                                let previous_field = metric_value_field(metric.value());
                                set_metric_type(metric, &value)?;
                                refresh_cached_metric_value(metric, metric_value, previous_field);
                            }
                            _ => {
                                return Err(MetricPathError::InvalidPath {
                                    path: &path.to_string(),
                                    expected: VALID_METRIC_PATHS_SET,
                                }
                                .to_string());
                            }
                        }

                        metric_value.insert(path, value);

                        return Ok(());
                    }

                    Err(MetricPathError::InvalidPath {
                        path: &path.to_string(),
                        expected: VALID_METRIC_PATHS_SET,
                    }
                    .to_string())
                }
            },
            PathPrefix::Metadata => {
                self.metadata_mut()
                    .value_mut()
                    .insert(&target_path.path, value);
                Ok(())
            }
        }
    }

    fn target_get(&self, target_path: &OwnedTargetPath) -> Result<Option<&Value>, String> {
        match target_path.prefix {
            PathPrefix::Event => match self {
                VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => {
                    Ok(log.get(&target_path.path))
                }
                VrlTarget::Metric { value, .. } => target_get_metric(&target_path.path, value),
            },
            PathPrefix::Metadata => Ok(self.metadata().value().get(&target_path.path)),
        }
    }

    fn target_get_mut(
        &mut self,
        target_path: &OwnedTargetPath,
    ) -> Result<Option<&mut Value>, String> {
        match target_path.prefix {
            PathPrefix::Event => match self {
                VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => {
                    Ok(log.get_mut(&target_path.path))
                }
                VrlTarget::Metric { value, .. } => target_get_mut_metric(&target_path.path, value),
            },
            PathPrefix::Metadata => Ok(self.metadata_mut().value_mut().get_mut(&target_path.path)),
        }
    }

    fn target_remove(
        &mut self,
        target_path: &OwnedTargetPath,
        compact: bool,
    ) -> Result<Option<vrl::value::Value>, String> {
        match target_path.prefix {
            PathPrefix::Event => match self {
                VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => {
                    Ok(log.remove(&target_path.path, compact))
                }
                VrlTarget::Metric {
                    metric,
                    value,
                    tag_mode,
                } => {
                    if target_path.path.is_root() {
                        return Err(MetricPathError::SetPathError.to_string());
                    }

                    if let Some(paths) = target_path
                        .path
                        .to_alternative_components(MAX_METRIC_PATH_DEPTH)
                    {
                        let removed_value = match paths.as_slice() {
                            ["namespace"] => metric.series.name.namespace.take().map(Into::into),
                            ["timestamp"] => metric.data.time.timestamp.take().map(Into::into),
                            ["interval_ms"] => metric
                                .data
                                .time
                                .interval_ms
                                .take()
                                .map(u32::from)
                                .map(Into::into),
                            ["tags"] => metric
                                .series
                                .tags
                                .take()
                                .map(|tags| metric_tags_to_vrl_value(tags, *tag_mode)),
                            ["tags", field] => metric
                                .remove_tag_set(field)
                                .map(|tag_set| tag_value_set_to_vrl_value(&tag_set, *tag_mode)),
                            ["type"] => return Err(MetricPathError::RemoveTypeError.to_string()),
                            _ => {
                                return Err(MetricPathError::InvalidPath {
                                    path: &target_path.path.to_string(),
                                    expected: VALID_METRIC_PATHS_SET,
                                }
                                .to_string());
                            }
                        };

                        value.remove(&target_path.path, false);

                        Ok(removed_value)
                    } else {
                        Ok(None)
                    }
                }
            },
            PathPrefix::Metadata => Ok(self
                .metadata_mut()
                .value_mut()
                .remove(&target_path.path, compact)),
        }
    }
}

impl SecretTarget for VrlTarget {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.metadata().secrets().get_secret(key)
    }

    fn insert_secret(&mut self, key: &str, value: &str) {
        self.metadata_mut().secrets_mut().insert_secret(key, value);
    }

    fn remove_secret(&mut self, key: &str) {
        self.metadata_mut().secrets_mut().remove_secret(key);
    }
}

/// Retrieves a value from a the provided metric using the path.
/// Currently the root path and the following paths are supported:
/// - `name`
/// - `namespace`
/// - `interval_ms`
/// - `timestamp`
/// - `kind`
/// - `tags`
/// - `tags.<tagname>`
/// - `type`
///
/// Any other paths result in a `MetricPathError::InvalidPath` being returned.
fn target_get_metric<'a>(
    path: &OwnedValuePath,
    value: &'a Value,
) -> Result<Option<&'a Value>, String> {
    if path.is_root() {
        return Ok(Some(value));
    }

    let value = value.get(path);

    if is_metric_value_path(path) {
        return Ok(value);
    }

    let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH) else {
        return Ok(None);
    };

    match paths.as_slice() {
        ["name" | "kind" | "type" | "namespace" | "timestamp" | "interval_ms" | "tags"]
        | ["tags", _] => Ok(value),
        _ => Err(MetricPathError::InvalidPath {
            path: &path.to_string(),
            expected: VALID_METRIC_PATHS_GET,
        }
        .to_string()),
    }
}

fn target_get_mut_metric<'a>(
    path: &OwnedValuePath,
    value: &'a mut Value,
) -> Result<Option<&'a mut Value>, String> {
    if path.is_root() {
        return Ok(Some(value));
    }

    let value = value.get_mut(path);

    let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH) else {
        return Ok(None);
    };

    match paths.as_slice() {
        ["name" | "kind" | "namespace" | "timestamp" | "interval_ms" | "tags"] | ["tags", _] => {
            Ok(value)
        }
        _ => Err(MetricPathError::InvalidPath {
            path: &path.to_string(),
            expected: VALID_METRIC_PATHS_SET,
        }
        .to_string()),
    }
}

struct MetricProperty {
    property: &'static str,
    getter: fn(&Metric) -> Option<Value>,
    set: bool,
}

impl MetricProperty {
    fn new(property: &'static str, getter: fn(&Metric) -> Option<Value>) -> Self {
        Self {
            property,
            getter,
            set: false,
        }
    }

    fn insert(&mut self, metric: &Metric, map: &mut ObjectMap) {
        if self.set {
            return;
        }
        if let Some(value) = (self.getter)(metric) {
            map.insert(self.property.into(), value);
            self.set = true;
        }
    }
}

fn get_single_value_tags(metric: &Metric) -> Option<Value> {
    metric
        .tags()
        .cloned()
        .map(|tags| metric_tags_to_vrl_value(tags, MetricTagMode::Single))
}

fn get_multi_value_tags(metric: &Metric) -> Option<Value> {
    metric
        .tags()
        .cloned()
        .map(|tags| metric_tags_to_vrl_value(tags, MetricTagMode::Full))
}

/// `Auto` keeps the underlying shape: a tag with exactly one value renders
/// as a scalar (string for `Value(_)`, null for `Bare`), and a tag with two
/// or more values renders as an array. The discriminator must be `len()` --
/// `as_single()` deliberately collapses multi-value sets to their last
/// value to support `Single`-mode semantics, which is the opposite of what
/// we want here.
fn get_auto_value_tags(metric: &Metric) -> Option<Value> {
    metric
        .tags()
        .cloned()
        .map(|tags| metric_tags_to_vrl_value(tags, MetricTagMode::Auto))
}

fn tag_ref_to_vrl_value(value: Option<&str>) -> Value {
    value.map_or(Value::Null, Value::from)
}

fn tag_value_set_to_vrl_value(tag_set: &TagValueSet, tag_mode: MetricTagMode) -> Value {
    match tag_mode {
        MetricTagMode::Single => tag_set.as_single().map_or(Value::Null, Value::from),
        MetricTagMode::Full => Value::Array(tag_set.iter().map(tag_ref_to_vrl_value).collect()),
        MetricTagMode::Auto => match tag_set.len() {
            1 => match tag_set.iter().next() {
                Some(Some(s)) => Value::from(s),
                _ => Value::Null,
            },
            _ => Value::Array(tag_set.iter().map(tag_ref_to_vrl_value).collect()),
        },
    }
}

fn metric_tags_to_vrl_value(tags: MetricTags, tag_mode: MetricTagMode) -> Value {
    match tag_mode {
        MetricTagMode::Single => tags
            .into_iter_single()
            .map(|(tag, value)| (tag.into(), value.into()))
            .collect::<ObjectMap>()
            .into(),
        MetricTagMode::Full | MetricTagMode::Auto => tags
            .iter_sets()
            .map(|(tag, tag_set)| (tag.into(), tag_value_set_to_vrl_value(tag_set, tag_mode)))
            .collect::<ObjectMap>()
            .into(),
    }
}

/// Returns the field that holds the value of a metric of this type.
const fn metric_value_field(value: &MetricValue) -> &'static str {
    match value {
        MetricValue::Counter { .. } => "counter",
        MetricValue::Gauge { .. } => "gauge",
        MetricValue::Set { .. } => "set",
        MetricValue::Distribution { .. } => "distribution",
        MetricValue::AggregatedHistogram { .. } => "aggregated_histogram",
        MetricValue::AggregatedSummary { .. } => "aggregated_summary",
        MetricValue::Sketch { .. } => "sketch",
    }
}

/// Returns `true` if the path reads a metric's value, such as `.gauge.value`.
fn is_metric_value_path(path: &OwnedValuePath) -> bool {
    matches!(
        path.segments.first(),
        Some(OwnedSegment::Field(field)) if METRIC_VALUE_FIELDS.contains(&field.as_ref())
    )
}

/// Converts a float for VRL.
///
/// VRL floats can't hold NaN, so NaN is exposed as the string `"NaN"`. This matches how NaN is
/// written in the native JSON encoding of histogram buckets. Infinities are kept as floats.
fn metric_float_to_vrl_value(value: f64) -> Value {
    if value.is_nan() {
        Value::from("NaN")
    } else {
        Value::from_f64_or_zero(value)
    }
}

/// Converts an unsigned count for VRL.
///
/// VRL integers are signed 64-bit, so counts above `i64::MAX` are exposed as exact decimal
/// strings rather than being truncated or rounded to a float.
fn metric_count_to_vrl_value(count: u64) -> Value {
    i64::try_from(count).map_or_else(|_| Value::from(count.to_string()), Value::from)
}

fn optional_metric_float_to_vrl_value(value: Option<f64>) -> Value {
    value.map_or(Value::Null, metric_float_to_vrl_value)
}

fn sample_to_vrl_value(sample: &Sample) -> Value {
    ObjectMap::from([
        ("value".into(), metric_float_to_vrl_value(sample.value)),
        ("rate".into(), Value::from(i64::from(sample.rate))),
    ])
    .into()
}

fn bucket_to_vrl_value(bucket: &Bucket) -> Value {
    ObjectMap::from([
        (
            "upper_limit".into(),
            metric_float_to_vrl_value(bucket.upper_limit),
        ),
        ("count".into(), metric_count_to_vrl_value(bucket.count)),
    ])
    .into()
}

fn quantile_to_vrl_value(quantile: &Quantile) -> Value {
    ObjectMap::from([
        (
            "quantile".into(),
            metric_float_to_vrl_value(quantile.quantile),
        ),
        ("value".into(), metric_float_to_vrl_value(quantile.value)),
    ])
    .into()
}

/// Only summary statistics are exposed. The bins are an internal representation and shouldn't
/// become part of the VRL interface.
fn sketch_to_vrl_value(sketch: &MetricSketch) -> ObjectMap {
    let MetricSketch::AgentDDSketch(sketch) = sketch;
    ObjectMap::from([
        ("count".into(), Value::from(i64::from(sketch.count()))),
        (
            "min".into(),
            optional_metric_float_to_vrl_value(sketch.min()),
        ),
        (
            "max".into(),
            optional_metric_float_to_vrl_value(sketch.max()),
        ),
        (
            "sum".into(),
            optional_metric_float_to_vrl_value(sketch.sum()),
        ),
        (
            "avg".into(),
            optional_metric_float_to_vrl_value(sketch.avg()),
        ),
    ])
}

/// Converts a metric's value to the object VRL reads under the field for its type.
fn metric_value_to_vrl_value(value: &MetricValue) -> Value {
    let map: ObjectMap = match value {
        MetricValue::Counter { value } | MetricValue::Gauge { value } => {
            ObjectMap::from([("value".into(), metric_float_to_vrl_value(*value))])
        }
        MetricValue::Set { values } => ObjectMap::from([(
            "values".into(),
            Value::Array(values.iter().map(|value| value.as_str().into()).collect()),
        )]),
        MetricValue::Distribution { samples, statistic } => ObjectMap::from([
            (
                "samples".into(),
                Value::Array(samples.iter().map(sample_to_vrl_value).collect()),
            ),
            (
                "statistic".into(),
                match statistic {
                    StatisticKind::Histogram => "histogram",
                    StatisticKind::Summary => "summary",
                }
                .into(),
            ),
        ]),
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => ObjectMap::from([
            (
                "buckets".into(),
                Value::Array(buckets.iter().map(bucket_to_vrl_value).collect()),
            ),
            ("count".into(), metric_count_to_vrl_value(*count)),
            ("sum".into(), optional_metric_float_to_vrl_value(*sum)),
        ]),
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => ObjectMap::from([
            (
                "quantiles".into(),
                Value::Array(quantiles.iter().map(quantile_to_vrl_value).collect()),
            ),
            ("count".into(), metric_count_to_vrl_value(*count)),
            ("sum".into(), metric_float_to_vrl_value(*sum)),
        ]),
        MetricValue::Sketch { sketch } => sketch_to_vrl_value(sketch),
    };
    map.into()
}

/// Updates the cached value after a metric's type changed from `previous_field`.
///
/// The value is only cached when the program reads it, so nothing is added otherwise.
fn refresh_cached_metric_value(metric: &Metric, cached: &mut Value, previous_field: &str) {
    if let Value::Object(map) = cached
        && map.remove(previous_field).is_some()
    {
        map.insert(
            metric_value_field(metric.value()).into(),
            metric_value_to_vrl_value(metric.value()),
        );
    }
}

/// Returns `true` if the path is `.type`.
fn is_metric_type_path(target_path: &OwnedTargetPath) -> bool {
    target_path.prefix == PathPrefix::Event
        && matches!(target_path.path.segments.as_slice(), [OwnedSegment::Field(field)] if field.as_ref() == "type")
}

/// Returns `true` if the path reads one of the arrays in the value of the current type, such as
/// `.distribution.samples`.
fn is_metric_value_array_path(target_path: &OwnedTargetPath, value_field: &str) -> bool {
    target_path.prefix == PathPrefix::Event
        && matches!(
            target_path.path.segments.as_slice(),
            [OwnedSegment::Field(field), OwnedSegment::Field(array), ..]
                if field.as_ref() == value_field
                    && matches!(array.as_ref(), "values" | "samples" | "buckets" | "quantiles")
        )
}

/// pre-compute the `Value` structure of the metric.
///
/// This structure is partially populated based on the fields accessed by
/// the VRL program as informed by `ProgramInfo`.
fn precompute_metric_value(metric: &Metric, info: &ProgramInfo, tag_mode: MetricTagMode) -> Value {
    let mut name = MetricProperty::new("name", |metric| Some(metric.name().to_owned().into()));
    let mut kind = MetricProperty::new("kind", |metric| Some(metric.kind().into()));
    let mut type_ = MetricProperty::new("type", |metric| Some(metric.value().clone().into()));
    let mut namespace = MetricProperty::new("namespace", |metric| {
        metric.namespace().map(String::from).map(Into::into)
    });
    let mut interval_ms =
        MetricProperty::new("interval_ms", |metric| metric.interval_ms().map(Into::into));
    let mut timestamp =
        MetricProperty::new("timestamp", |metric| metric.timestamp().map(Into::into));
    let mut tags = MetricProperty::new(
        "tags",
        match tag_mode {
            MetricTagMode::Single => get_single_value_tags,
            MetricTagMode::Full => get_multi_value_tags,
            MetricTagMode::Auto => get_auto_value_tags,
        },
    );

    let mut map = ObjectMap::default();
    let value_field = metric_value_field(metric.value());
    // A counter or gauge can be converted to the other type with `.type`. In that case, reading
    // the field of either type needs the value, so that it can be moved on conversion.
    let convertible = matches!(
        metric.value(),
        MetricValue::Counter { .. } | MetricValue::Gauge { .. }
    ) && info.target_assignments.iter().any(is_metric_type_path);
    let mut value_inserted = false;
    let mut insert_value = |map: &mut ObjectMap| {
        if !value_inserted {
            map.insert(
                value_field.into(),
                metric_value_to_vrl_value(metric.value()),
            );
            value_inserted = true;
        }
    };

    for target_path in &info.target_queries {
        // Accessing a root path requires us to pre-populate all fields. So does reading one of
        // the value's arrays, because `unnest` copies the whole event around each element without
        // registering a query for the root.
        if target_path == &OwnedTargetPath::event_root()
            || is_metric_value_array_path(target_path, value_field)
        {
            let mut properties = [
                &mut name,
                &mut kind,
                &mut type_,
                &mut namespace,
                &mut interval_ms,
                &mut timestamp,
                &mut tags,
            ];
            for property in &mut properties {
                property.insert(metric, &mut map);
            }
            insert_value(&mut map);
            break;
        }

        // For non-root paths, we continuously populate the value with the
        // relevant data.
        if let Some(OwnedSegment::Field(field)) = target_path.path.segments.first() {
            // Reading the field for the current type prepares the value. Fields for other types
            // read as `null`, so they don't, unless the type can be converted to them.
            if METRIC_VALUE_FIELDS.contains(&field.as_ref()) {
                if field.as_ref() == value_field
                    || (convertible && matches!(field.as_ref(), "counter" | "gauge"))
                {
                    insert_value(&mut map);
                }
                continue;
            }
            let property = match field.as_ref() {
                "name" => Some(&mut name),
                "kind" => Some(&mut kind),
                "type" => Some(&mut type_),
                "namespace" => Some(&mut namespace),
                "timestamp" => Some(&mut timestamp),
                "interval_ms" => Some(&mut interval_ms),
                "tags" => Some(&mut tags),
                _ => None,
            };
            if let Some(property) = property {
                property.insert(metric, &mut map);
            }
        }
    }

    map.into()
}

#[derive(Debug, Snafu)]
enum MetricPathError<'a> {
    #[snafu(display("cannot set root path"))]
    SetPathError,

    #[snafu(display("cannot delete metric type"))]
    RemoveTypeError,

    #[snafu(display("invalid path {}: expected one of {}", path, expected))]
    InvalidPath { path: &'a str, expected: &'a str },
}

#[cfg(test)]
mod test {
    use chrono::{Utc, offset::TimeZone};
    use lookup::owned_value_path;
    use similar_asserts::assert_eq;
    use vrl::{btreemap, event_path, objectmap, value::kind::Index};

    use super::*;
    use crate::{
        event::metric::{MetricSketch, StatisticKind},
        metric_tags,
        metrics::AgentDDSketch,
    };

    #[test]
    fn test_field_definitions_in_message() {
        let definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Legacy]);
        assert_eq!(
            Definition::new_with_default_metadata(
                Kind::object(BTreeMap::from([("message".into(), Kind::bytes())])),
                [LogNamespace::Legacy]
            ),
            move_field_definitions_into_message(definition)
        );

        // Test when a message field already exists.
        let definition = Definition::new_with_default_metadata(
            Kind::object(BTreeMap::from([("message".into(), Kind::integer())])).or_bytes(),
            [LogNamespace::Legacy],
        );
        assert_eq!(
            Definition::new_with_default_metadata(
                Kind::object(BTreeMap::from([(
                    "message".into(),
                    Kind::bytes().or_integer()
                )])),
                [LogNamespace::Legacy]
            ),
            move_field_definitions_into_message(definition)
        );
    }

    #[test]
    fn test_merged_array_definitions_simple() {
        // Test merging the array definitions where the schema definition
        // is simple, containing only one possible type in the array.
        let object: BTreeMap<vrl::value::kind::Field, Kind> = [
            ("carrot".into(), Kind::bytes()),
            ("potato".into(), Kind::integer()),
        ]
        .into();

        let kind = Kind::array(Collection::from_unknown(Kind::object(object)));

        let definition = Definition::new_with_default_metadata(kind, [LogNamespace::Legacy]);

        let kind = Kind::object(BTreeMap::from([
            ("carrot".into(), Kind::bytes()),
            ("potato".into(), Kind::integer()),
        ]));

        let wanted = Definition::new_with_default_metadata(kind, [LogNamespace::Legacy]);
        let merged = merge_array_definitions(definition);

        assert_eq!(wanted, merged);
    }

    #[test]
    fn test_merged_array_definitions_complex() {
        // Test merging the array definitions where the schema definition
        // is fairly complex containing multiple different possible types.
        let object: BTreeMap<vrl::value::kind::Field, Kind> = [
            ("carrot".into(), Kind::bytes()),
            ("potato".into(), Kind::integer()),
        ]
        .into();

        let array: BTreeMap<Index, Kind> = [
            (Index::from(0), Kind::integer()),
            (Index::from(1), Kind::boolean()),
            (
                Index::from(2),
                Kind::object(BTreeMap::from([("peas".into(), Kind::bytes())])),
            ),
        ]
        .into();

        let mut kind = Kind::bytes();
        kind.add_object(object);
        kind.add_array(array);

        let definition = Definition::new_with_default_metadata(kind, [LogNamespace::Legacy]);

        let mut kind = Kind::bytes();
        kind.add_integer();
        kind.add_boolean();
        kind.add_object(BTreeMap::from([
            ("carrot".into(), Kind::bytes().or_undefined()),
            ("potato".into(), Kind::integer().or_undefined()),
            ("peas".into(), Kind::bytes().or_undefined()),
        ]));

        let wanted = Definition::new_with_default_metadata(kind, [LogNamespace::Legacy]);
        let merged = merge_array_definitions(definition);

        assert_eq!(wanted, merged);
    }

    #[test]
    fn log_get() {
        let cases = vec![
            (
                ObjectMap::new(),
                owned_value_path!(),
                Ok(Some(ObjectMap::new().into())),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!(),
                Ok(Some(ObjectMap::from([("foo".into(), "bar".into())]).into())),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo"),
                Ok(Some("bar".into())),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("bar"),
                Ok(None),
            ),
            (
                objectmap! { "foo" => vec![objectmap! { "bar" => true }] },
                owned_value_path!("foo", 0, "bar"),
                Ok(Some(true.into())),
            ),
            (
                objectmap! { "foo" => objectmap! { "bar baz" => objectmap! { "baz" => 2 } } },
                owned_value_path!("foo", r"bar baz", "baz"),
                Ok(Some(2.into())),
            ),
        ];

        for (value, path, expect) in cases {
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let target = VrlTarget::new(
                Event::Log(LogEvent::from(value)),
                &info,
                MetricTagMode::Single,
            );
            let path = OwnedTargetPath::event(path);

            assert_eq!(
                Target::target_get(&target, &path).map(Option::<&Value>::cloned),
                expect
            );
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn log_insert() {
        let cases = vec![
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!(0),
                objectmap! { "baz" => "qux" }.into(),
                objectmap! { "baz" => "qux" },
                Ok(()),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo"),
                "baz".into(),
                objectmap! { "foo" => "baz" },
                Ok(()),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo", 2, "bar baz", "a", "b"),
                true.into(),
                objectmap! {
                    "foo" => vec![
                        Value::Null,
                        Value::Null,
                        objectmap! {
                            "bar baz" => objectmap! { "a" => objectmap! { "b" => true } },
                        }.into()
                    ]
                },
                Ok(()),
            ),
            (
                objectmap! { "foo" => vec![0, 1, 2] },
                owned_value_path!("foo", 5),
                "baz".into(),
                objectmap! {
                    "foo" => vec![
                        0.into(),
                        1.into(),
                        2.into(),
                        Value::Null,
                        Value::Null,
                        Value::from("baz"),
                    ],
                },
                Ok(()),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo", 0),
                "baz".into(),
                objectmap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                objectmap! { "foo" => Value::Array(vec![]) },
                owned_value_path!("foo", 0),
                "baz".into(),
                objectmap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                objectmap! { "foo" => Value::Array(vec![0.into()]) },
                owned_value_path!("foo", 0),
                "baz".into(),
                objectmap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                objectmap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                owned_value_path!("foo", 0),
                "baz".into(),
                objectmap! { "foo" => Value::Array(vec!["baz".into(), 1.into()]) },
                Ok(()),
            ),
            (
                objectmap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                owned_value_path!("foo", 1),
                "baz".into(),
                objectmap! { "foo" => Value::Array(vec![0.into(), "baz".into()]) },
                Ok(()),
            ),
        ];

        for (object, path, value, expect, result) in cases {
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(
                Event::Log(LogEvent::from(object)),
                &info,
                MetricTagMode::Single,
            );
            let expect = LogEvent::from(expect);
            let value: Value = value;
            let path = OwnedTargetPath::event(path);

            assert_eq!(
                Target::target_insert(&mut target, &path, value.clone()),
                result
            );
            assert_eq!(
                Target::target_get(&target, &path).map(Option::<&Value>::cloned),
                Ok(Some(value))
            );
            assert_eq!(
                match target.into_events(LogNamespace::Legacy) {
                    TargetEvents::One(event) => vec![event],
                    TargetEvents::Logs(events) => events.collect::<Vec<_>>(),
                    TargetEvents::Traces(events) => events.collect::<Vec<_>>(),
                }
                .first()
                .cloned()
                .unwrap(),
                Event::Log(expect)
            );
        }
    }

    #[test]
    fn log_remove() {
        let cases = vec![
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo"),
                false,
                Some(ObjectMap::new().into()),
            ),
            (
                ObjectMap::from([("foo".into(), "bar".into())]),
                owned_value_path!(r"foo bar", "foo"),
                false,
                Some(objectmap! { "foo" => "bar"}.into()),
            ),
            (
                objectmap! { "foo" => "bar", "baz" => "qux" },
                owned_value_path!(),
                false,
                Some(ObjectMap::new().into()),
            ),
            (
                objectmap! { "foo" => "bar", "baz" => "qux" },
                owned_value_path!(),
                true,
                Some(ObjectMap::new().into()),
            ),
            (
                objectmap! { "foo" => vec![0] },
                owned_value_path!("foo", 0),
                false,
                Some(objectmap! { "foo" => Value::Array(vec![]) }.into()),
            ),
            (
                objectmap! { "foo" => vec![0] },
                owned_value_path!("foo", 0),
                true,
                Some(ObjectMap::new().into()),
            ),
            (
                objectmap! {
                    "foo" => objectmap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                owned_value_path!("foo", r"bar baz", 0),
                false,
                Some(
                    objectmap! {
                        "foo" => objectmap! { "bar baz" => Value::Array(vec![]) },
                        "bar" => "baz",
                    }
                    .into(),
                ),
            ),
            (
                objectmap! {
                    "foo" => objectmap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                owned_value_path!("foo", r"bar baz", 0),
                true,
                Some(objectmap! { "bar" => "baz" }.into()),
            ),
        ];

        for (object, path, compact, expect) in cases {
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(
                Event::Log(LogEvent::from(object)),
                &info,
                MetricTagMode::Single,
            );
            let path = OwnedTargetPath::event(path);
            let removed = Target::target_get(&target, &path).unwrap().cloned();

            assert_eq!(
                Target::target_remove(&mut target, &path, compact),
                Ok(removed)
            );
            assert_eq!(
                Target::target_get(&target, &OwnedTargetPath::event_root())
                    .map(Option::<&Value>::cloned),
                Ok(expect)
            );
        }
    }

    #[test]
    fn log_into_events() {
        let cases = vec![
            (
                Value::from(objectmap! {"foo" => "bar"}),
                vec![objectmap! {"foo" => "bar"}],
            ),
            (Value::from(1), vec![objectmap! {"message" => 1}]),
            (Value::from("2"), vec![objectmap! {"message" => "2"}]),
            (Value::from(true), vec![objectmap! {"message" => true}]),
            (
                Value::from(vec![
                    Value::from(1),
                    Value::from("2"),
                    Value::from(true),
                    Value::from(objectmap! {"foo" => "bar"}),
                ]),
                vec![
                    objectmap! {"message" => 1},
                    objectmap! {"message" => "2"},
                    objectmap! {"message" => true},
                    objectmap! {"foo" => "bar"},
                ],
            ),
        ];

        for (value, expect) in cases {
            let metadata = EventMetadata::default();
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(
                Event::Log(LogEvent::new_with_metadata(metadata.clone())),
                &info,
                MetricTagMode::Single,
            );

            Target::target_insert(&mut target, &OwnedTargetPath::event_root(), value).unwrap();

            assert_eq!(
                match target.into_events(LogNamespace::Legacy) {
                    TargetEvents::One(event) => vec![event],
                    TargetEvents::Logs(events) => events.collect::<Vec<_>>(),
                    TargetEvents::Traces(events) => events.collect::<Vec<_>>(),
                },
                expect
                    .into_iter()
                    .map(|v| Event::Log(LogEvent::from_map(v, metadata.clone())))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn trace_scalar_root_into_log_drops_trace_layout() {
        let mut trace = TraceEvent::from(objectmap! {"foo" => "bar"});
        trace
            .metadata_mut()
            .set_trace_layout(super::super::TraceLayout::Datadog);

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Trace(trace), &info, MetricTagMode::Single);
        Target::target_insert(
            &mut target,
            &OwnedTargetPath::event_root(),
            Value::from("message"),
        )
        .unwrap();

        match target.into_events(LogNamespace::Legacy) {
            TargetEvents::One(Event::Log(log)) => {
                assert_eq!(log.namespace(), LogNamespace::Legacy);
                assert_eq!(log.metadata().trace_layout(), None);
                assert_eq!(
                    log.get(event_path!("message")),
                    Some(&Value::from("message"))
                );
            }
            _ => panic!("expected one log event"),
        }
    }

    #[test]
    fn metric_all_fields() {
        let metric = Metric::new(
            "zub",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_namespace(Some("zoob"))
        .with_tags(Some(metric_tags!("tig" => "tog")))
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2020, 12, 10, 12, 0, 0)
                .single()
                .expect("invalid timestamp"),
        ))
        .with_interval_ms(Some(NonZero::<u32>::new(507).unwrap()));

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![
                OwnedTargetPath::event(owned_value_path!("name")),
                OwnedTargetPath::event(owned_value_path!("namespace")),
                OwnedTargetPath::event(owned_value_path!("interval_ms")),
                OwnedTargetPath::event(owned_value_path!("timestamp")),
                OwnedTargetPath::event(owned_value_path!("kind")),
                OwnedTargetPath::event(owned_value_path!("type")),
                OwnedTargetPath::event(owned_value_path!("tags")),
            ],
            target_assignments: vec![],
        };
        let target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Single);

        assert_eq!(
            Ok(Some(
                btreemap! {
                    "name" => "zub",
                    "namespace" => "zoob",
                    "interval_ms" => 507,
                    "timestamp" => Utc.with_ymd_and_hms(2020, 12, 10, 12, 0, 0).single().expect("invalid timestamp"),
                    "tags" => btreemap! { "tig" => "tog" },
                    "kind" => "absolute",
                    "type" => "counter",
                }
                .into()
            )),
            target
                .target_get(&OwnedTargetPath::event_root())
                .map(Option::<&Value>::cloned)
        );
    }

    #[test]
    fn metric_fields() {
        struct Case {
            path: OwnedValuePath,
            current: Option<Value>,
            new: Value,
            delete: bool,
        }

        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_tags(Some(metric_tags!("tig" => "tog")));

        let cases = vec![
            Case {
                path: owned_value_path!("name"),
                current: Some(Value::from("name")),
                new: Value::from("namefoo"),
                delete: false,
            },
            Case {
                path: owned_value_path!("namespace"),
                current: None,
                new: "namespacefoo".into(),
                delete: true,
            },
            Case {
                path: owned_value_path!("timestamp"),
                current: None,
                new: Utc
                    .with_ymd_and_hms(2020, 12, 8, 12, 0, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into(),
                delete: true,
            },
            Case {
                path: owned_value_path!("interval_ms"),
                current: None,
                new: 123_456.into(),
                delete: true,
            },
            Case {
                path: owned_value_path!("kind"),
                current: Some(Value::from("absolute")),
                new: "incremental".into(),
                delete: false,
            },
            Case {
                path: owned_value_path!("tags", "thing"),
                current: None,
                new: "footag".into(),
                delete: true,
            },
        ];

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![
                OwnedTargetPath::event(owned_value_path!("name")),
                OwnedTargetPath::event(owned_value_path!("namespace")),
                OwnedTargetPath::event(owned_value_path!("timestamp")),
                OwnedTargetPath::event(owned_value_path!("interval_ms")),
                OwnedTargetPath::event(owned_value_path!("kind")),
            ],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Single);

        for Case {
            path,
            current,
            new,
            delete,
        } in cases
        {
            let path = OwnedTargetPath::event(path);

            assert_eq!(
                Ok(current),
                target.target_get(&path).map(Option::<&Value>::cloned)
            );
            assert_eq!(Ok(()), target.target_insert(&path, new.clone()));
            assert_eq!(
                Ok(Some(new.clone())),
                target.target_get(&path).map(Option::<&Value>::cloned)
            );

            if delete {
                assert_eq!(Ok(Some(new)), target.target_remove(&path, true));
                assert_eq!(
                    Ok(None),
                    target.target_get(&path).map(Option::<&Value>::cloned)
                );
            }
        }
    }

    #[test]
    fn metric_set_tags() {
        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_tags(Some(metric_tags!("tig" => "tog")));

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Single);
        let _result = target.target_insert(
            &OwnedTargetPath::event(owned_value_path!("tags")),
            Value::Object(ObjectMap::from([("a".into(), "b".into())])),
        );

        match target {
            VrlTarget::Metric {
                metric,
                value: _,
                tag_mode: _,
            } => {
                assert!(metric.tags().is_some());
                assert_eq!(metric.tags().unwrap(), &crate::metric_tags!("a" => "b"));
            }
            _ => panic!("must be a metric"),
        }
    }

    #[test]
    fn metric_invalid_paths() {
        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        );

        let validpaths_get = [
            ".name",
            ".namespace",
            ".interval_ms",
            ".timestamp",
            ".kind",
            ".tags",
            ".type",
            ".counter",
            ".gauge",
            ".set",
            ".distribution",
            ".aggregated_histogram",
            ".aggregated_summary",
            ".sketch",
        ];

        let validpaths_set = [
            ".name",
            ".namespace",
            ".interval_ms",
            ".timestamp",
            ".kind",
            ".tags",
            ".type",
        ];

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Single);

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_get.join(", ")
            )),
            target.target_get(&OwnedTargetPath::event(owned_value_path!("zork")))
        );

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            target.target_insert(
                &OwnedTargetPath::event(owned_value_path!("zork")),
                "thing".into()
            )
        );

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            target.target_remove(&OwnedTargetPath::event(owned_value_path!("zork")), true)
        );

        assert_eq!(
            Err(format!(
                "invalid path tags.foo.flork: expected one of {}",
                validpaths_get.join(", ")
            )),
            target.target_get(&OwnedTargetPath::event(owned_value_path!(
                "tags", "foo", "flork"
            )))
        );
    }

    fn type_program_info() -> ProgramInfo {
        ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![OwnedTargetPath::event(owned_value_path!("type"))],
            target_assignments: vec![],
        }
    }

    fn set_type(metric: Metric, new_type: Value) -> (VrlTarget, Result<(), String>) {
        let mut target = VrlTarget::new(
            Event::Metric(metric),
            &type_program_info(),
            MetricTagMode::Single,
        );
        let result =
            target.target_insert(&OwnedTargetPath::event(owned_value_path!("type")), new_type);
        (target, result)
    }

    fn into_metric(target: VrlTarget) -> Metric {
        match target.into_events(LogNamespace::Legacy) {
            TargetEvents::One(Event::Metric(metric)) => metric,
            _ => panic!("must be a metric"),
        }
    }

    #[test]
    fn metric_set_type_converts_between_counter_and_gauge() {
        let cases = [
            (
                MetricValue::Gauge { value: 4.5 },
                "counter",
                MetricValue::Counter { value: 4.5 },
            ),
            (
                MetricValue::Counter { value: 4.5 },
                "gauge",
                MetricValue::Gauge { value: 4.5 },
            ),
            (
                MetricValue::Counter { value: 4.5 },
                "counter",
                MetricValue::Counter { value: 4.5 },
            ),
            (
                MetricValue::Gauge { value: 4.5 },
                "gauge",
                MetricValue::Gauge { value: 4.5 },
            ),
        ];

        for kind in [MetricKind::Absolute, MetricKind::Incremental] {
            for (value, new_type, expected) in cases.clone() {
                let metric =
                    Metric::new("name", kind, value).with_tags(Some(metric_tags!("host" => "a")));
                let (target, result) = set_type(metric, new_type.into());
                assert_eq!(result, Ok(()));

                // Later reads in the same program see the new type.
                assert_eq!(
                    target.target_get(&OwnedTargetPath::event(owned_value_path!("type"))),
                    Ok(Some(&Value::from(new_type)))
                );

                let metric = into_metric(target);
                assert_eq!(metric.value(), &expected);
                assert_eq!(metric.kind(), kind);
                assert_eq!(metric.name(), "name");
                assert_eq!(metric.tag_value("host"), Some("a".into()));
            }
        }
    }

    #[test]
    fn metric_set_type_rejects_invalid_values() {
        for new_type in [
            Value::from("distribution"),
            Value::from("Counter"),
            Value::from(""),
            Value::from(1),
            Value::Null,
        ] {
            let metric = Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1.0 },
            );
            let (target, result) = set_type(metric, new_type.clone());
            assert!(result.is_err(), "{new_type:?} should be rejected");
            assert_eq!(
                target.target_get(&OwnedTargetPath::event(owned_value_path!("type"))),
                Ok(Some(&Value::from("gauge"))),
                "{new_type:?} should leave `.type` reading the original type"
            );
            assert_eq!(
                into_metric(target).value(),
                &MetricValue::Gauge { value: 1.0 },
                "{new_type:?} should leave the metric unchanged"
            );
        }

        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        );
        assert_eq!(
            set_type(metric, "set".into()).1,
            Err("invalid metric type set, metric type must be `counter` or `gauge`".to_owned())
        );
    }

    #[test]
    fn metric_set_type_rejects_non_scalar_metrics() {
        let values = [
            MetricValue::Set {
                values: ["a".to_owned()].into(),
            },
            MetricValue::Distribution {
                samples: crate::samples![1.0 => 1],
                statistic: StatisticKind::Histogram,
            },
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![1.0 => 1],
                count: 1,
                sum: Some(1.0),
            },
            MetricValue::AggregatedSummary {
                quantiles: crate::quantiles![0.5 => 1.0],
                count: 1,
                sum: 1.0,
            },
            MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(AgentDDSketch::with_agent_defaults()),
            },
        ];

        for value in values {
            let name = value.as_name();
            for new_type in ["counter", "gauge", name] {
                let metric = Metric::new("name", MetricKind::Absolute, value.clone());
                let (target, result) = set_type(metric, new_type.into());
                assert_eq!(
                    result,
                    Err(format!(
                        "cannot change the type of a {name} metric, only `counter` and `gauge` metrics can be converted"
                    ))
                );
                assert_eq!(into_metric(target).value(), &value);
            }
        }
    }

    #[test]
    fn metric_remove_type_is_an_error() {
        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        let mut target = VrlTarget::new(
            Event::Metric(metric),
            &type_program_info(),
            MetricTagMode::Single,
        );

        assert_eq!(
            target.target_remove(&OwnedTargetPath::event(owned_value_path!("type")), true),
            Err("cannot delete metric type".to_owned())
        );
        assert_eq!(
            into_metric(target).value(),
            &MetricValue::Counter { value: 1.0 }
        );
    }

    #[test]
    fn test_metric_insert_get_multi_value_tag() {
        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        );
        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };

        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Full);

        let value = Value::Array(vec!["a".into(), "".into(), Value::Null, "b".into()]);
        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("tags", "foo")),
                value,
            )
            .unwrap();

        let vrl_tags_value = target
            .target_get(&OwnedTargetPath::event(owned_value_path!("tags")))
            .unwrap()
            .unwrap();

        assert_eq!(
            vrl_tags_value,
            &Value::Object(ObjectMap::from([(
                "foo".into(),
                Value::Array(vec!["a".into(), "".into(), Value::Null, "b".into()])
            )]))
        );

        let VrlTarget::Metric { metric, .. } = target else {
            unreachable!()
        };

        // get single value (should be the last one)
        assert_eq!(metric.tag_value("foo"), Some("b".into()));
    }

    fn auto_mode_program_info() -> ProgramInfo {
        ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![OwnedTargetPath::event(owned_value_path!("tags"))],
            target_assignments: vec![],
        }
    }

    /// Auto exposes a single-value tag as a plain string -- the same shape
    /// `Single` would produce -- so existing VRL programs that read scalar
    /// tags work unchanged.
    #[test]
    fn metric_auto_read_single_value_tag_returns_string() {
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(metric_tags!("env" => "prod")));

        let info = auto_mode_program_info();
        let target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        let tags = target
            .target_get(&OwnedTargetPath::event(owned_value_path!("tags")))
            .unwrap()
            .unwrap()
            .clone();

        assert_eq!(
            tags,
            Value::Object(ObjectMap::from([("env".into(), Value::from("prod"))])),
        );
    }

    /// Auto exposes a multi-value tag as an array -- the same shape `Full`
    /// would produce -- so multi-value information survives the round trip
    /// instead of being collapsed to the last value.
    #[test]
    fn metric_auto_read_multi_value_tag_returns_array() {
        use super::super::metric::{MetricTags, TagValueSet};
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(MetricTags(BTreeMap::from([(
            "shard".to_string(),
            TagValueSet::from(vec![
                TagValue::from("a".to_string()),
                TagValue::from("b".to_string()),
            ]),
        )]))));

        let info = auto_mode_program_info();
        let target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        let tags = target
            .target_get(&OwnedTargetPath::event(owned_value_path!("tags")))
            .unwrap()
            .unwrap()
            .clone();

        assert_eq!(
            tags,
            Value::Object(ObjectMap::from([(
                "shard".into(),
                Value::Array(vec!["a".into(), "b".into()])
            )])),
        );
    }

    /// Auto reads a mixed metric (one single, one multi) preserving both
    /// shapes -- the property the new mode exists for.
    #[test]
    fn metric_auto_read_mixed_tags_preserves_both_shapes() {
        use super::super::metric::{MetricTags, TagValueSet};
        let mut tags = BTreeMap::new();
        tags.insert(
            "env".to_string(),
            TagValueSet::from(vec!["prod".to_string()]),
        );
        tags.insert(
            "shard".to_string(),
            TagValueSet::from(vec!["a".to_string(), "b".to_string()]),
        );
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(MetricTags(tags)));

        let info = auto_mode_program_info();
        let target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        let tags = target
            .target_get(&OwnedTargetPath::event(owned_value_path!("tags")))
            .unwrap()
            .unwrap()
            .clone();

        assert_eq!(
            tags,
            Value::Object(ObjectMap::from([
                ("env".into(), Value::from("prod")),
                ("shard".into(), Value::Array(vec!["a".into(), "b".into()])),
            ])),
        );
    }

    /// Auto write: assigning a scalar produces a single-value tag (so a VRL
    /// program that did `.tags.env = "staging"` round-trips as `Single`-style).
    #[test]
    fn metric_auto_write_string_creates_single_tag() {
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        let info = auto_mode_program_info();
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("tags", "env")),
                Value::from("staging"),
            )
            .unwrap();

        let VrlTarget::Metric { metric, .. } = target else {
            unreachable!()
        };
        let metric_tags = metric.tags().expect("tag should be set");
        let tag_set = metric_tags
            .iter_sets()
            .find(|(k, _)| *k == "env")
            .expect("env tag missing")
            .1;
        assert_eq!(
            tag_set.len(),
            1,
            "scalar assignment must produce a 1-element set"
        );
        assert_eq!(tag_set.as_single(), Some("staging"));
    }

    /// Auto write: assigning an array produces a multi-value tag (the
    /// behaviour `Single` would silently drop and `Full` would force on
    /// every other tag).
    #[test]
    fn metric_auto_write_array_creates_multi_value_tag() {
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        let info = auto_mode_program_info();
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("tags", "shard")),
                Value::Array(vec!["a".into(), "b".into(), "c".into()]),
            )
            .unwrap();

        let VrlTarget::Metric { metric, .. } = target else {
            unreachable!()
        };
        let metric_tags = metric.tags().expect("tag should be set");
        let tag_set = metric_tags
            .iter_sets()
            .find(|(k, _)| *k == "shard")
            .expect("shard tag missing")
            .1;
        assert_eq!(tag_set.len(), 3);
        let collected: Vec<Option<&str>> = tag_set.iter().collect();
        assert_eq!(
            collected,
            vec![Some("a"), Some("b"), Some("c")],
            "array assignment must preserve every element"
        );
    }

    /// Auto round-trip across two passes: a length-1 array write reaches the
    /// metric storage as `TagValueSet::Single` (the storage layer never holds
    /// a `Set` with fewer than 2 elements), so when a *subsequent* VRL pass
    /// constructs a new `VrlTarget` around that metric, the tag surfaces as a
    /// scalar string rather than a 1-element array. This pins that behaviour
    /// as intentional -- a true array-shape preserving round-trip needs
    /// `MetricTagMode::Full`.
    ///
    /// Within a single pass, the `VrlTarget` caches writes verbatim in its
    /// precomputed `Value`, so an intra-pass `.tags.region` read sees the
    /// original array; the collapse is observable only across passes.
    #[test]
    fn metric_auto_one_element_array_normalises_to_scalar() {
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        let info = auto_mode_program_info();
        let mut first_pass = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        first_pass
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("tags", "region")),
                Value::Array(vec!["us-east-1".into()]),
            )
            .unwrap();

        let VrlTarget::Metric { metric, .. } = first_pass else {
            unreachable!()
        };
        let tag_set = metric
            .tags()
            .expect("tag should be set")
            .iter_sets()
            .find(|(k, _)| *k == "region")
            .expect("region tag missing")
            .1;
        assert_eq!(
            tag_set.len(),
            1,
            "storage layer must collapse a 1-element multi-value tag to a Single",
        );

        let second_pass = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);
        let tags = second_pass
            .target_get(&OwnedTargetPath::event(owned_value_path!("tags")))
            .unwrap()
            .unwrap()
            .clone();

        assert_eq!(
            tags,
            Value::Object(ObjectMap::from([(
                "region".into(),
                Value::from("us-east-1"),
            )])),
            "a length-1 array written in one Auto pass must read back as a \
             scalar on the next pass",
        );
    }

    /// Auto read: an empty tag set (`len() == 0`) keeps its array shape so a
    /// noop `.tags = .tags` round-trip preserves it. Without this guarantee
    /// an empty multi-value tag would silently morph into a bare single-value
    /// tag through one Auto round-trip.
    #[test]
    fn metric_auto_read_empty_tag_set_returns_array() {
        use super::super::metric::{MetricTags, TagValueSet};
        let mut tags = BTreeMap::new();
        tags.insert("empty".to_string(), TagValueSet::default());
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(MetricTags(tags)));

        let info = auto_mode_program_info();
        let target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        let tags = target
            .target_get(&OwnedTargetPath::event(owned_value_path!("tags")))
            .unwrap()
            .unwrap()
            .clone();

        assert_eq!(
            tags,
            Value::Object(ObjectMap::from([("empty".into(), Value::Array(vec![]))])),
            "empty tag set must surface as an empty array, not as null",
        );
    }

    /// Auto write: assigning `null` falls through the scalar path, producing
    /// a single bare tag (matches `Single` semantics so the two modes agree
    /// on the unambiguous case).
    #[test]
    fn metric_auto_write_null_creates_single_bare_tag() {
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        let info = auto_mode_program_info();
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("tags", "bare")),
                Value::Null,
            )
            .unwrap();

        let VrlTarget::Metric { metric, .. } = target else {
            unreachable!()
        };
        let metric_tags = metric.tags().expect("tag should be set");
        let tag_set = metric_tags
            .iter_sets()
            .find(|(k, _)| *k == "bare")
            .expect("bare tag missing")
            .1;
        assert_eq!(tag_set.len(), 1);
        let collected: Vec<Option<&str>> = tag_set.iter().collect();
        assert_eq!(
            collected,
            vec![None],
            "null must produce a single bare value"
        );
    }

    /// `del(.tags.<field>)` must return the same shape that a read would have
    /// exposed, so moving multi-value tags in `Auto` mode does not silently
    /// drop values.
    #[test]
    fn metric_auto_remove_multi_value_tag_returns_array() {
        use super::super::metric::{MetricTags, TagValue, TagValueSet};
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(MetricTags(BTreeMap::from([(
            "shard".to_string(),
            TagValueSet::from(vec![
                TagValue::from("a".to_string()),
                TagValue::from("b".to_string()),
            ]),
        )]))));

        let info = auto_mode_program_info();
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        let removed = target
            .target_remove(
                &OwnedTargetPath::event(owned_value_path!("tags", "shard")),
                true,
            )
            .unwrap()
            .expect("shard tag should be removed");

        assert_eq!(removed, Value::Array(vec!["a".into(), "b".into()]));

        let VrlTarget::Metric { metric, .. } = target else {
            unreachable!()
        };
        assert!(metric.tags().is_none());
    }

    /// `del(.tags)` in `Auto` mode must preserve mixed single/multi shapes.
    #[test]
    fn metric_auto_remove_all_tags_preserves_shapes() {
        use super::super::metric::{MetricTags, TagValueSet};
        let mut tags = BTreeMap::new();
        tags.insert(
            "env".to_string(),
            TagValueSet::from(vec!["prod".to_string()]),
        );
        tags.insert(
            "shard".to_string(),
            TagValueSet::from(vec!["a".to_string(), "b".to_string()]),
        );
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(MetricTags(tags)));

        let info = auto_mode_program_info();
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Auto);

        let removed = target
            .target_remove(&OwnedTargetPath::event(owned_value_path!("tags")), true)
            .unwrap()
            .expect("tags should be removed");

        assert_eq!(
            removed,
            Value::Object(ObjectMap::from([
                ("env".into(), Value::from("prod")),
                ("shard".into(), Value::Array(vec!["a".into(), "b".into()]))
            ]))
        );
    }

    /// `del(.tags.<field>)` in `Full` mode must return the full value array.
    #[test]
    fn metric_full_remove_multi_value_tag_returns_array() {
        use super::super::metric::{MetricTags, TagValue, TagValueSet};
        let metric = Metric::new(
            "m",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(MetricTags(BTreeMap::from([(
            "shard".to_string(),
            TagValueSet::from(vec![
                TagValue::from("a".to_string()),
                TagValue::from("b".to_string()),
            ]),
        )]))));

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Full);

        let removed = target
            .target_remove(
                &OwnedTargetPath::event(owned_value_path!("tags", "shard")),
                true,
            )
            .unwrap()
            .expect("shard tag should be removed");

        assert_eq!(removed, Value::Array(vec!["a".into(), "b".into()]));
    }
    fn metric_target_with_queries(value: MetricValue, queries: &[OwnedTargetPath]) -> VrlTarget {
        let metric = Metric::new("m", MetricKind::Absolute, value);
        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: queries.to_vec(),
            target_assignments: vec![],
        };
        VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Single)
    }

    fn get(target: &VrlTarget, path: OwnedValuePath) -> Option<Value> {
        target
            .target_get(&OwnedTargetPath::event(path))
            .unwrap()
            .cloned()
    }

    #[test]
    fn metric_value_read_by_type() {
        let mut sketch = AgentDDSketch::with_agent_defaults();
        sketch.insert_many(&[1.0, 3.0]);

        let cases = [
            (
                MetricValue::Counter { value: 1.5 },
                "counter",
                Value::from(objectmap! { "value" => 1.5 }),
            ),
            (
                MetricValue::Gauge { value: -2.0 },
                "gauge",
                Value::from(objectmap! { "value" => -2.0 }),
            ),
            (
                MetricValue::Set {
                    values: ["b".to_owned(), "a".to_owned()].into(),
                },
                "set",
                Value::from(objectmap! { "values" => vec!["a", "b"] }),
            ),
            (
                MetricValue::Distribution {
                    samples: crate::samples![1.0 => 2, 4.0 => 1],
                    statistic: StatisticKind::Summary,
                },
                "distribution",
                Value::from(objectmap! {
                    "samples" => vec![
                        Value::from(objectmap! { "value" => 1.0, "rate" => 2 }),
                        Value::from(objectmap! { "value" => 4.0, "rate" => 1 }),
                    ],
                    "statistic" => "summary",
                }),
            ),
            (
                MetricValue::AggregatedHistogram {
                    buckets: crate::buckets![1.0 => 2, f64::INFINITY => 3],
                    count: 5,
                    sum: Some(7.5),
                },
                "aggregated_histogram",
                Value::from(objectmap! {
                    "buckets" => vec![
                        Value::from(objectmap! { "upper_limit" => 1.0, "count" => 2 }),
                        Value::from(objectmap! { "upper_limit" => f64::INFINITY, "count" => 3 }),
                    ],
                    "count" => 5,
                    "sum" => 7.5,
                }),
            ),
            (
                MetricValue::AggregatedSummary {
                    quantiles: crate::quantiles![0.5 => 2.0, 0.99 => 3.0],
                    count: 4,
                    sum: 9.0,
                },
                "aggregated_summary",
                Value::from(objectmap! {
                    "quantiles" => vec![
                        Value::from(objectmap! { "quantile" => 0.5, "value" => 2.0 }),
                        Value::from(objectmap! { "quantile" => 0.99, "value" => 3.0 }),
                    ],
                    "count" => 4,
                    "sum" => 9.0,
                }),
            ),
            (
                MetricValue::Sketch {
                    sketch: MetricSketch::AgentDDSketch(sketch.clone()),
                },
                "sketch",
                Value::from(objectmap! {
                    "count" => 2,
                    "min" => sketch.min().unwrap(),
                    "max" => sketch.max().unwrap(),
                    "sum" => sketch.sum().unwrap(),
                    "avg" => sketch.avg().unwrap(),
                }),
            ),
        ];

        for (value, field, expected) in cases {
            let queries: Vec<_> = METRIC_VALUE_FIELDS
                .iter()
                .map(|field| OwnedTargetPath::event(owned_value_path!(*field)))
                .collect();
            let target = metric_target_with_queries(value, &queries);

            assert_eq!(
                get(&target, owned_value_path!(field)),
                Some(expected),
                "{field}"
            );

            // The fields for other types are missing, so they read as `null`.
            for other in METRIC_VALUE_FIELDS.iter().filter(|other| **other != field) {
                assert_eq!(get(&target, owned_value_path!(*other)), None, "{field}");
            }
        }
    }

    #[test]
    fn metric_value_read_nested_paths() {
        let target = metric_target_with_queries(
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![1.0 => 2],
                count: 2,
                sum: None,
            },
            &[OwnedTargetPath::event(owned_value_path!(
                "aggregated_histogram",
                "buckets"
            ))],
        );

        assert_eq!(
            get(
                &target,
                owned_value_path!("aggregated_histogram", "buckets", 0, "count")
            ),
            Some(Value::from(2))
        );
        // A missing sum reads as `null`, which is different from a sum of zero.
        assert_eq!(
            get(&target, owned_value_path!("aggregated_histogram", "sum")),
            Some(Value::Null)
        );
    }

    #[test]
    fn metric_value_read_special_numbers() {
        let query = |field: &str| vec![OwnedTargetPath::event(owned_value_path!(field))];

        // VRL floats can't hold NaN, so it reads as the string "NaN".
        let target =
            metric_target_with_queries(MetricValue::Gauge { value: f64::NAN }, &query("gauge"));
        assert_eq!(
            get(&target, owned_value_path!("gauge", "value")),
            Some(Value::from("NaN"))
        );

        // Infinities stay floats.
        let target = metric_target_with_queries(
            MetricValue::Gauge {
                value: f64::NEG_INFINITY,
            },
            &query("gauge"),
        );
        assert_eq!(
            get(&target, owned_value_path!("gauge", "value")),
            Some(Value::from(f64::NEG_INFINITY))
        );

        // Counts that don't fit in a VRL integer read as exact decimal strings.
        let big = u64::MAX;
        let target = metric_target_with_queries(
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![1.0 => big],
                count: big,
                sum: Some(1.0),
            },
            &query("aggregated_histogram"),
        );
        assert_eq!(
            get(&target, owned_value_path!("aggregated_histogram", "count")),
            Some(Value::from("18446744073709551615"))
        );
        assert_eq!(
            get(
                &target,
                owned_value_path!("aggregated_histogram", "buckets", 0, "count")
            ),
            Some(Value::from("18446744073709551615"))
        );

        let target = metric_target_with_queries(
            MetricValue::AggregatedSummary {
                quantiles: crate::quantiles![0.5 => f64::NAN],
                count: i64::MAX as u64 + 1,
                sum: 1.0,
            },
            &query("aggregated_summary"),
        );
        assert_eq!(
            get(&target, owned_value_path!("aggregated_summary", "count")),
            Some(Value::from("9223372036854775808"))
        );
        assert_eq!(
            get(
                &target,
                owned_value_path!("aggregated_summary", "quantiles", 0, "value")
            ),
            Some(Value::from("NaN"))
        );

        // The largest count that fits stays an integer.
        let target = metric_target_with_queries(
            MetricValue::AggregatedSummary {
                quantiles: vec![],
                count: i64::MAX as u64,
                sum: 1.0,
            },
            &query("aggregated_summary"),
        );
        assert_eq!(
            get(&target, owned_value_path!("aggregated_summary", "count")),
            Some(Value::from(i64::MAX))
        );
    }

    #[test]
    fn metric_value_read_root_includes_value() {
        let target = metric_target_with_queries(
            MetricValue::Counter { value: 1.0 },
            &[OwnedTargetPath::event_root()],
        );
        let root = target
            .target_get(&OwnedTargetPath::event_root())
            .unwrap()
            .cloned()
            .unwrap();
        assert_eq!(
            root.get(&owned_value_path!("counter")).cloned(),
            Some(Value::from(objectmap! { "value" => 1.0 }))
        );
    }

    #[test]
    fn metric_value_not_computed_unless_read() {
        let target = metric_target_with_queries(
            MetricValue::Counter { value: 1.0 },
            &[OwnedTargetPath::event(owned_value_path!("name"))],
        );
        let VrlTarget::Metric { value, .. } = &target else {
            panic!("must be a metric");
        };
        assert_eq!(value.get(&owned_value_path!("counter")), None);
    }

    #[test]
    fn metric_value_not_computed_for_other_types() {
        // `exists(.gauge)` on a large distribution shouldn't convert every sample.
        let target = metric_target_with_queries(
            MetricValue::Distribution {
                samples: crate::samples![1.0 => 1, 2.0 => 1],
                statistic: StatisticKind::Histogram,
            },
            &[OwnedTargetPath::event(owned_value_path!("gauge"))],
        );
        assert_eq!(get(&target, owned_value_path!("gauge")), None);
        let VrlTarget::Metric { value, .. } = &target else {
            panic!("must be a metric");
        };
        assert_eq!(value.get(&owned_value_path!("distribution")), None);
    }

    #[test]
    fn metric_value_array_read_prepares_whole_event() {
        // `unnest` reads the root without registering a query for it, so reading an array in the
        // value prepares every field.
        let target = metric_target_with_queries(
            MetricValue::Distribution {
                samples: crate::samples![1.0 => 1],
                statistic: StatisticKind::Histogram,
            },
            &[OwnedTargetPath::event(owned_value_path!(
                "distribution",
                "samples"
            ))],
        );
        assert_eq!(
            get(&target, owned_value_path!("name")),
            Some(Value::from("m"))
        );
        assert_eq!(
            get(&target, owned_value_path!("type")),
            Some(Value::from("distribution"))
        );
    }

    #[test]
    fn metric_value_conversion_reading_only_new_type() {
        let metric = Metric::new("m", MetricKind::Absolute, MetricValue::Gauge { value: 3.0 });
        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![OwnedTargetPath::event(owned_value_path!(
                "counter", "value"
            ))],
            target_assignments: vec![OwnedTargetPath::event(owned_value_path!("type"))],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, MetricTagMode::Single);
        assert_eq!(get(&target, owned_value_path!("counter")), None);

        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("type")),
                "counter".into(),
            )
            .unwrap();

        assert_eq!(
            get(&target, owned_value_path!("counter", "value")),
            Some(Value::from(3.0))
        );
    }

    #[test]
    fn metric_value_follows_type_conversion() {
        let queries = [
            OwnedTargetPath::event(owned_value_path!("type")),
            OwnedTargetPath::event(owned_value_path!("counter", "value")),
            OwnedTargetPath::event(owned_value_path!("gauge", "value")),
            OwnedTargetPath::event_root(),
        ];
        let mut target = metric_target_with_queries(MetricValue::Gauge { value: 3.0 }, &queries);

        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("type")),
                "counter".into(),
            )
            .unwrap();

        assert_eq!(
            get(&target, owned_value_path!("counter", "value")),
            Some(Value::from(3.0))
        );
        assert_eq!(get(&target, owned_value_path!("gauge")), None);
        let root = target
            .target_get(&OwnedTargetPath::event_root())
            .unwrap()
            .cloned()
            .unwrap();
        assert_eq!(root.get(&owned_value_path!("gauge")), None);
        assert_eq!(
            root.get(&owned_value_path!("counter")).cloned(),
            Some(Value::from(objectmap! { "value" => 3.0 }))
        );
        assert_eq!(
            root.get(&owned_value_path!("type")).cloned(),
            Some(Value::from("counter"))
        );
    }

    #[test]
    fn metric_value_write_still_rejected() {
        let mut target = metric_target_with_queries(
            MetricValue::Gauge { value: 1.0 },
            &[OwnedTargetPath::event(owned_value_path!("gauge"))],
        );

        target
            .target_insert(
                &OwnedTargetPath::event(owned_value_path!("gauge", "value")),
                2.0.into(),
            )
            .unwrap_err();
        target
            .target_remove(&OwnedTargetPath::event(owned_value_path!("gauge")), false)
            .unwrap_err();

        assert_eq!(
            get(&target, owned_value_path!("gauge", "value")),
            Some(Value::from(1.0))
        );
        let VrlTarget::Metric { metric, .. } = &target else {
            panic!("must be a metric");
        };
        assert_eq!(metric.value(), &MetricValue::Gauge { value: 1.0 });
    }
}
