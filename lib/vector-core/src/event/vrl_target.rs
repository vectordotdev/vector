use std::borrow::Cow;
use std::{collections::BTreeMap, convert::TryFrom, marker::PhantomData};

use lookup::lookup_v2::OwnedSegment;
use lookup::{OwnedTargetPath, OwnedValuePath, PathPrefix};
use snafu::Snafu;
use vrl::compiler::value::VrlValueConvert;
use vrl::compiler::{ProgramInfo, SecretTarget, Target};
use vrl::prelude::Collection;
use vrl::value::{Kind, ObjectMap, Value};

use super::{metric::TagValue, Event, EventMetadata, LogEvent, Metric, MetricKind, TraceEvent};
use crate::config::{log_schema, LogNamespace};
use crate::schema::Definition;

const VALID_METRIC_PATHS_SET: &str = ".name, .namespace, .timestamp, .kind, .tags";

/// We can get the `type` of the metric in Remap, but can't set it.
const VALID_METRIC_PATHS_GET: &str = ".name, .namespace, .timestamp, .kind, .tags, .type";

/// Metrics aren't interested in paths that have a length longer than 3.
///
/// The longest path is 2, and we need to check that a third segment doesn't exist as we don't want
/// fields such as `.tags.host.thing`.
const MAX_METRIC_PATH_DEPTH: usize = 3;

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
        multi_value_tags: bool,
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
    pub fn new(event: Event, info: &ProgramInfo, multi_value_metric_tags: bool) -> Self {
        match event {
            Event::Log(event) => {
                let (value, metadata) = event.into_parts();
                VrlTarget::LogEvent(value, metadata)
            }
            Event::Metric(metric) => {
                // We pre-generate [`Value`] types for the metric fields accessed in
                // the event. This allows us to then return references to those
                // values, even if the field is accessed more than once.
                let value = precompute_metric_value(&metric, info);

                VrlTarget::Metric {
                    metric,
                    value,
                    multi_value_tags: multi_value_metric_tags,
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
            VrlTarget::Trace(value, metadata) => match value {
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

                v => TargetEvents::One(create_log_event(v, metadata).into()),
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

    if !message.is_never() {
        if let Some(message_key) = log_schema().message_key() {
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

fn set_metric_tag_values(name: String, value: &Value, metric: &mut Metric, multi_value_tags: bool) {
    if multi_value_tags {
        let tag_values = value
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter_map(|value| match value {
                Value::Bytes(bytes) => {
                    Some(TagValue::Value(String::from_utf8_lossy(bytes).to_string()))
                }
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

impl Target for VrlTarget {
    fn target_insert(&mut self, target_path: &OwnedTargetPath, value: Value) -> Result<(), String> {
        let path = &target_path.path;
        match target_path.prefix {
            PathPrefix::Event => match self {
                VrlTarget::LogEvent(ref mut log, _) | VrlTarget::Trace(ref mut log, _) => {
                    log.insert(path, value);
                    Ok(())
                }
                VrlTarget::Metric {
                    ref mut metric,
                    value: metric_value,
                    multi_value_tags,
                } => {
                    if path.is_root() {
                        return Err(MetricPathError::SetPathError.to_string());
                    }

                    if let Some(paths) = path
                        .to_alternative_components(MAX_METRIC_PATH_DEPTH)
                        .first()
                    {
                        match paths.as_slice() {
                            ["tags"] => {
                                let value =
                                    value.clone().try_object().map_err(|e| e.to_string())?;

                                metric.remove_tags();
                                for (field, value) in &value {
                                    set_metric_tag_values(
                                        field[..].into(),
                                        value,
                                        metric,
                                        *multi_value_tags,
                                    );
                                }
                            }
                            ["tags", field] => {
                                set_metric_tag_values(
                                    (*field).to_owned(),
                                    &value,
                                    metric,
                                    *multi_value_tags,
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
                            ["timestamp"] => {
                                let value =
                                    value.clone().try_timestamp().map_err(|e| e.to_string())?;
                                metric.data.time.timestamp = Some(value);
                            }
                            ["kind"] => {
                                metric.data.kind = MetricKind::try_from(value.clone())?;
                            }
                            _ => {
                                return Err(MetricPathError::InvalidPath {
                                    path: &path.to_string(),
                                    expected: VALID_METRIC_PATHS_SET,
                                }
                                .to_string())
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

    #[allow(clippy::redundant_closure_for_method_calls)] // false positive
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
                VrlTarget::LogEvent(ref mut log, _) | VrlTarget::Trace(ref mut log, _) => {
                    Ok(log.remove(&target_path.path, compact))
                }
                VrlTarget::Metric {
                    ref mut metric,
                    value,
                    multi_value_tags: _,
                } => {
                    if target_path.path.is_root() {
                        return Err(MetricPathError::SetPathError.to_string());
                    }

                    if let Some(paths) = target_path
                        .path
                        .to_alternative_components(MAX_METRIC_PATH_DEPTH)
                        .first()
                    {
                        let removed_value = match paths.as_slice() {
                            ["namespace"] => metric.series.name.namespace.take().map(Into::into),
                            ["timestamp"] => metric.data.time.timestamp.take().map(Into::into),
                            ["tags"] => metric.series.tags.take().map(|map| {
                                map.into_iter_single()
                                    .map(|(k, v)| (k, v.into()))
                                    .collect::<vrl::value::Value>()
                            }),
                            ["tags", field] => metric.remove_tag(field).map(Into::into),
                            _ => {
                                return Err(MetricPathError::InvalidPath {
                                    path: &target_path.path.to_string(),
                                    expected: VALID_METRIC_PATHS_SET,
                                }
                                .to_string())
                            }
                        };

                        value.remove(&target_path.path, false);

                        return Ok(removed_value);
                    }

                    Ok(None)
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
/// - name
/// - namespace
/// - timestamp
/// - kind
/// - tags
/// - tags.<tagname>
/// - type
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

    for paths in path.to_alternative_components(MAX_METRIC_PATH_DEPTH) {
        match paths.as_slice() {
            ["name"] | ["kind"] | ["type"] | ["tags", _] => return Ok(value),
            ["namespace"] | ["timestamp"] | ["tags"] => {
                if let Some(value) = value {
                    return Ok(Some(value));
                }
            }
            _ => {
                return Err(MetricPathError::InvalidPath {
                    path: &path.to_string(),
                    expected: VALID_METRIC_PATHS_GET,
                }
                .to_string())
            }
        }
    }

    // We only reach this point if we have requested a tag that doesn't exist or an empty
    // field.
    Ok(None)
}

fn target_get_mut_metric<'a>(
    path: &OwnedValuePath,
    value: &'a mut Value,
) -> Result<Option<&'a mut Value>, String> {
    if path.is_root() {
        return Ok(Some(value));
    }

    let value = value.get_mut(path);

    for paths in path.to_alternative_components(MAX_METRIC_PATH_DEPTH) {
        match paths.as_slice() {
            ["name"] | ["kind"] | ["tags", _] => return Ok(value),
            ["namespace"] | ["timestamp"] | ["tags"] => {
                if let Some(value) = value {
                    return Ok(Some(value));
                }
            }
            _ => {
                return Err(MetricPathError::InvalidPath {
                    path: &path.to_string(),
                    expected: VALID_METRIC_PATHS_SET,
                }
                .to_string())
            }
        }
    }

    // We only reach this point if we have requested a tag that doesn't exist or an empty
    // field.
    Ok(None)
}

/// pre-compute the `Value` structure of the metric.
///
/// This structure is partially populated based on the fields accessed by
/// the VRL program as informed by `ProgramInfo`.
fn precompute_metric_value(metric: &Metric, info: &ProgramInfo) -> Value {
    let mut map = ObjectMap::default();

    let mut set_name = false;
    let mut set_kind = false;
    let mut set_type = false;
    let mut set_namespace = false;
    let mut set_timestamp = false;
    let mut set_tags = false;

    for target_path in &info.target_queries {
        // Accessing a root path requires us to pre-populate all fields.
        if target_path == &OwnedTargetPath::event_root() {
            if !set_name {
                map.insert("name".into(), metric.name().to_owned().into());
            }

            if !set_kind {
                map.insert("kind".into(), metric.kind().into());
            }

            if !set_type {
                map.insert("type".into(), metric.value().clone().into());
            }

            if !set_namespace {
                if let Some(namespace) = metric.namespace() {
                    map.insert("namespace".into(), namespace.to_owned().into());
                }
            }

            if !set_timestamp {
                if let Some(timestamp) = metric.timestamp() {
                    map.insert("timestamp".into(), timestamp.into());
                }
            }

            if !set_tags {
                if let Some(tags) = metric.tags().cloned() {
                    map.insert(
                        "tags".into(),
                        tags.into_iter_single()
                            .map(|(tag, value)| (tag.into(), value.into()))
                            .collect::<ObjectMap>()
                            .into(),
                    );
                }
            }

            break;
        }

        // For non-root paths, we continuously populate the value with the
        // relevant data.
        if let Some(OwnedSegment::Field(field)) = target_path.path.segments.first() {
            match field.as_ref() {
                "name" if !set_name => {
                    set_name = true;
                    map.insert("name".into(), metric.name().to_owned().into());
                }
                "kind" if !set_kind => {
                    set_kind = true;
                    map.insert("kind".into(), metric.kind().into());
                }
                "type" if !set_type => {
                    set_type = true;
                    map.insert("type".into(), metric.value().clone().into());
                }
                "namespace" if !set_namespace && metric.namespace().is_some() => {
                    set_namespace = true;
                    map.insert(
                        "namespace".into(),
                        metric.namespace().unwrap().to_owned().into(),
                    );
                }
                "timestamp" if !set_timestamp && metric.timestamp().is_some() => {
                    set_timestamp = true;
                    map.insert("timestamp".into(), metric.timestamp().unwrap().into());
                }
                "tags" if !set_tags && metric.tags().is_some() => {
                    set_tags = true;
                    map.insert(
                        "tags".into(),
                        metric
                            .tags()
                            .cloned()
                            .unwrap()
                            .into_iter_single()
                            .map(|(tag, value)| (tag.into(), value.into()))
                            .collect::<ObjectMap>()
                            .into(),
                    );
                }
                _ => {}
            }
        }
    }

    map.into()
}

#[derive(Debug, Snafu)]
enum MetricPathError<'a> {
    #[snafu(display("cannot set root path"))]
    SetPathError,

    #[snafu(display("invalid path {}: expected one of {}", path, expected))]
    InvalidPath { path: &'a str, expected: &'a str },
}

#[cfg(test)]
mod test {
    use chrono::{offset::TimeZone, Utc};
    use lookup::owned_value_path;
    use similar_asserts::assert_eq;
    use vrl::btreemap;
    use vrl::value::kind::Index;

    use super::super::MetricValue;
    use super::*;
    use crate::metric_tags;

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
                BTreeMap::new(),
                owned_value_path!(),
                Ok(Some(BTreeMap::new().into())),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!(),
                Ok(Some(BTreeMap::from([("foo".into(), "bar".into())]).into())),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo"),
                Ok(Some("bar".into())),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("bar"),
                Ok(None),
            ),
            (
                btreemap! { "foo" => vec![btreemap! { "bar" => true }] },
                owned_value_path!("foo", 0, "bar"),
                Ok(Some(true.into())),
            ),
            (
                btreemap! { "foo" => btreemap! { "bar baz" => btreemap! { "baz" => 2 } } },
                owned_value_path!("foo", r"bar baz", "baz"),
                Ok(Some(2.into())),
            ),
        ];

        for (value, path, expect) in cases {
            let value: ObjectMap = value;
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let target = VrlTarget::new(Event::Log(LogEvent::from(value)), &info, false);
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
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!(0),
                btreemap! { "baz" => "qux" }.into(),
                btreemap! { "baz" => "qux" },
                Ok(()),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo"),
                "baz".into(),
                btreemap! { "foo" => "baz" },
                Ok(()),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo", 2, "bar baz", "a", "b"),
                true.into(),
                btreemap! {
                    "foo" => vec![
                        Value::Null,
                        Value::Null,
                        btreemap! {
                            "bar baz" => btreemap! { "a" => btreemap! { "b" => true } },
                        }.into()
                    ]
                },
                Ok(()),
            ),
            (
                btreemap! { "foo" => vec![0, 1, 2] },
                owned_value_path!("foo", 5),
                "baz".into(),
                btreemap! {
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
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo", 0),
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![]) },
                owned_value_path!("foo", 0),
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into()]) },
                owned_value_path!("foo", 0),
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                owned_value_path!("foo", 0),
                "baz".into(),
                btreemap! { "foo" => Value::Array(vec!["baz".into(), 1.into()]) },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                owned_value_path!("foo", 1),
                "baz".into(),
                btreemap! { "foo" => Value::Array(vec![0.into(), "baz".into()]) },
                Ok(()),
            ),
        ];

        for (object, path, value, expect, result) in cases {
            let object: ObjectMap = object;
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(Event::Log(LogEvent::from(object)), &info, false);
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
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!("foo"),
                false,
                Some(BTreeMap::new().into()),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                owned_value_path!(r"foo bar", "foo"),
                false,
                Some(btreemap! { "foo" => "bar"}.into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                owned_value_path!(),
                false,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                owned_value_path!(),
                true,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! { "foo" => vec![0] },
                owned_value_path!("foo", 0),
                false,
                Some(btreemap! { "foo" => Value::Array(vec![]) }.into()),
            ),
            (
                btreemap! { "foo" => vec![0] },
                owned_value_path!("foo", 0),
                true,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! {
                    "foo" => btreemap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                owned_value_path!("foo", r"bar baz", 0),
                false,
                Some(
                    btreemap! {
                        "foo" => btreemap! { "bar baz" => Value::Array(vec![]) },
                        "bar" => "baz",
                    }
                    .into(),
                ),
            ),
            (
                btreemap! {
                    "foo" => btreemap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                owned_value_path!("foo", r"bar baz", 0),
                true,
                Some(btreemap! { "bar" => "baz" }.into()),
            ),
        ];

        for (object, path, compact, expect) in cases {
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(Event::Log(LogEvent::from(object)), &info, false);
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
        use vrl::btreemap;

        let cases = vec![
            (
                Value::from(btreemap! {"foo" => "bar"}),
                vec![btreemap! {"foo" => "bar"}],
            ),
            (Value::from(1), vec![btreemap! {"message" => 1}]),
            (Value::from("2"), vec![btreemap! {"message" => "2"}]),
            (Value::from(true), vec![btreemap! {"message" => true}]),
            (
                Value::from(vec![
                    Value::from(1),
                    Value::from("2"),
                    Value::from(true),
                    Value::from(btreemap! {"foo" => "bar"}),
                ]),
                vec![
                    btreemap! {"message" => 1},
                    btreemap! {"message" => "2"},
                    btreemap! {"message" => true},
                    btreemap! {"foo" => "bar"},
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
                false,
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
        ));

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![
                OwnedTargetPath::event(owned_value_path!("name")),
                OwnedTargetPath::event(owned_value_path!("namespace")),
                OwnedTargetPath::event(owned_value_path!("timestamp")),
                OwnedTargetPath::event(owned_value_path!("kind")),
                OwnedTargetPath::event(owned_value_path!("type")),
                OwnedTargetPath::event(owned_value_path!("tags")),
            ],
            target_assignments: vec![],
        };
        let target = VrlTarget::new(Event::Metric(metric), &info, false);

        assert_eq!(
            Ok(Some(
                btreemap! {
                    "name" => "zub",
                    "namespace" => "zoob",
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
        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_tags(Some(metric_tags!("tig" => "tog")));

        let cases = vec![
            (
                owned_value_path!("name"), // Path
                Some(Value::from("name")), // Current value
                Value::from("namefoo"),    // New value
                false,                     // Test deletion
            ),
            (
                owned_value_path!("namespace"),
                None,
                "namespacefoo".into(),
                true,
            ),
            (
                owned_value_path!("timestamp"),
                None,
                Utc.with_ymd_and_hms(2020, 12, 8, 12, 0, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into(),
                true,
            ),
            (
                owned_value_path!("kind"),
                Some(Value::from("absolute")),
                "incremental".into(),
                false,
            ),
            (
                owned_value_path!("tags", "thing"),
                None,
                "footag".into(),
                true,
            ),
        ];

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![
                OwnedTargetPath::event(owned_value_path!("name")),
                OwnedTargetPath::event(owned_value_path!("namespace")),
                OwnedTargetPath::event(owned_value_path!("timestamp")),
                OwnedTargetPath::event(owned_value_path!("kind")),
            ],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, false);

        for (path, current, new, delete) in cases {
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
        let mut target = VrlTarget::new(Event::Metric(metric), &info, false);
        let _result = target.target_insert(
            &OwnedTargetPath::event(owned_value_path!("tags")),
            Value::Object(BTreeMap::from([("a".into(), "b".into())])),
        );

        match target {
            VrlTarget::Metric {
                metric,
                value: _,
                multi_value_tags: _,
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
            ".timestamp",
            ".kind",
            ".tags",
            ".type",
        ];

        let validpaths_set = [".name", ".namespace", ".timestamp", ".kind", ".tags"];

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info, false);

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

        let mut target = VrlTarget::new(Event::Metric(metric), &info, true);

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
            &Value::Object(BTreeMap::from([(
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
}
