use std::{collections::BTreeMap, convert::TryFrom, marker::PhantomData};

use lookup::{LookupBuf, SegmentBuf};
use snafu::Snafu;
use vrl_lib::{prelude::VrlValueConvert, MetadataTarget, ProgramInfo, SecretTarget};

use super::{Event, EventMetadata, LogEvent, Metric, MetricKind, TraceEvent, Value};
use crate::config::log_schema;

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
    Metric { metric: Metric, value: Value },
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
}

impl Iterator for TargetIter<LogEvent> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|v| {
            match v {
                value @ Value::Object(_) => LogEvent::from_parts(value, self.metadata.clone()),
                value => {
                    let mut log = LogEvent::new_with_metadata(self.metadata.clone());
                    log.insert(log_schema().message_key(), value);
                    log
                }
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
                value => {
                    let mut log = LogEvent::new_with_metadata(self.metadata.clone());
                    log.insert(log_schema().message_key(), value);
                    TraceEvent::from(log)
                }
            }
            .into()
        })
    }
}

impl VrlTarget {
    pub fn new(event: Event, info: &ProgramInfo) -> Self {
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

                VrlTarget::Metric { metric, value }
            }
            Event::Trace(event) => {
                let (fields, metadata) = event.into_parts();
                VrlTarget::Trace(Value::Object(fields), metadata)
            }
        }
    }

    /// Turn the target back into events.
    ///
    /// This returns an iterator of events as one event can be turned into multiple by assigning an
    /// array to `.` in VRL.
    pub fn into_events(self) -> TargetEvents {
        match self {
            VrlTarget::LogEvent(value, metadata) => match value {
                value @ Value::Object(_) => {
                    TargetEvents::One(LogEvent::from_parts(value, metadata).into())
                }

                Value::Array(values) => TargetEvents::Logs(TargetIter {
                    iter: values.into_iter(),
                    metadata,
                    _marker: PhantomData,
                }),

                v => {
                    let mut log = LogEvent::new_with_metadata(metadata);
                    log.insert(log_schema().message_key(), v);
                    TargetEvents::One(log.into())
                }
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
                }),

                v => {
                    let mut log = LogEvent::new_with_metadata(metadata);
                    log.insert(log_schema().message_key(), v);
                    TargetEvents::One(log.into())
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

impl vrl_lib::Target for VrlTarget {
    fn target_insert(&mut self, path: &LookupBuf, value: ::value::Value) -> Result<(), String> {
        match self {
            VrlTarget::LogEvent(ref mut log, _) | VrlTarget::Trace(ref mut log, _) => {
                log.insert_by_path(path, value);
                Ok(())
            }
            VrlTarget::Metric {
                ref mut metric,
                value: metric_value,
            } => {
                if path.is_root() {
                    return Err(MetricPathError::SetPathError.to_string());
                }

                if let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH).get(0) {
                    match paths.as_slice() {
                        ["tags"] => {
                            let value = value.clone().try_object().map_err(|e| e.to_string())?;
                            for (field, value) in &value {
                                metric.insert_tag(
                                    field.as_str().to_owned(),
                                    value
                                        .try_bytes_utf8_lossy()
                                        .map_err(|e| e.to_string())?
                                        .into_owned(),
                                );
                            }
                        }
                        ["tags", field] => {
                            let value = value.clone().try_bytes().map_err(|e| e.to_string())?;
                            metric.insert_tag(
                                (*field).to_owned(),
                                String::from_utf8_lossy(&value).into_owned(),
                            );
                        }
                        ["name"] => {
                            let value = value.clone().try_bytes().map_err(|e| e.to_string())?;
                            metric.series.name.name = String::from_utf8_lossy(&value).into_owned();
                        }
                        ["namespace"] => {
                            let value = value.clone().try_bytes().map_err(|e| e.to_string())?;
                            metric.series.name.namespace =
                                Some(String::from_utf8_lossy(&value).into_owned());
                        }
                        ["timestamp"] => {
                            let value = value.clone().try_timestamp().map_err(|e| e.to_string())?;
                            metric.data.timestamp = Some(value);
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

                    metric_value.insert_by_path(path, value);

                    return Ok(());
                }

                Err(MetricPathError::InvalidPath {
                    path: &path.to_string(),
                    expected: VALID_METRIC_PATHS_SET,
                }
                .to_string())
            }
        }
    }

    #[allow(clippy::redundant_closure_for_method_calls)] // false positive
    fn target_get(&self, path: &LookupBuf) -> Result<Option<&Value>, String> {
        match self {
            VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => Ok(log.get_by_path(path)),
            VrlTarget::Metric { value, .. } => target_get_metric(path, value),
        }
    }

    fn target_get_mut(&mut self, path: &LookupBuf) -> Result<Option<&mut Value>, String> {
        match self {
            VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => Ok(log.get_by_path_mut(path)),
            VrlTarget::Metric { value, .. } => target_get_mut_metric(path, value),
        }
    }

    fn target_remove(
        &mut self,
        path: &LookupBuf,
        compact: bool,
    ) -> Result<Option<::value::Value>, String> {
        match self {
            VrlTarget::LogEvent(ref mut log, _) | VrlTarget::Trace(ref mut log, _) => {
                Ok(log.remove_by_path(path, compact))
            }
            VrlTarget::Metric {
                ref mut metric,
                value,
            } => {
                if path.is_root() {
                    return Err(MetricPathError::SetPathError.to_string());
                }

                if let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH).get(0) {
                    let removed_value = match paths.as_slice() {
                        ["namespace"] => metric.series.name.namespace.take().map(Into::into),
                        ["timestamp"] => metric.data.timestamp.take().map(Into::into),
                        ["tags"] => metric.series.tags.take().map(|map| {
                            map.into_iter()
                                .map(|(k, v)| (k, v.into()))
                                .collect::<::value::Value>()
                        }),
                        ["tags", field] => metric.remove_tag(field).map(Into::into),
                        _ => {
                            return Err(MetricPathError::InvalidPath {
                                path: &path.to_string(),
                                expected: VALID_METRIC_PATHS_SET,
                            }
                            .to_string())
                        }
                    };

                    value.remove_by_path(path, false);

                    return Ok(removed_value);
                }

                Ok(None)
            }
        }
    }
}

impl MetadataTarget for VrlTarget {
    fn get_metadata(&self, path: &LookupBuf) -> Result<Option<::value::Value>, String> {
        let value = self.metadata().value().get_by_path(path).cloned();
        Ok(value)
    }

    fn set_metadata(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.metadata_mut().value_mut().insert_by_path(path, value);
        Ok(())
    }

    fn remove_metadata(&mut self, path: &LookupBuf) -> Result<(), String> {
        self.metadata_mut().value_mut().remove_by_path(path, true);
        Ok(())
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
fn target_get_metric<'a>(path: &LookupBuf, value: &'a Value) -> Result<Option<&'a Value>, String> {
    if path.is_root() {
        return Ok(Some(value));
    }

    let value = value.get_by_path(path);

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
    path: &LookupBuf,
    value: &'a mut Value,
) -> Result<Option<&'a mut Value>, String> {
    if path.is_root() {
        return Ok(Some(value));
    }

    let value = value.get_by_path_mut(path);

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
    let mut map = BTreeMap::default();

    let mut set_name = false;
    let mut set_kind = false;
    let mut set_type = false;
    let mut set_namespace = false;
    let mut set_timestamp = false;
    let mut set_tags = false;

    for path in &info.target_queries {
        // Accessing a root path requires us to pre-populate all fields.
        if path.is_root() {
            if !set_name {
                map.insert("name".to_owned(), metric.name().to_owned().into());
            }

            if !set_kind {
                map.insert("kind".to_owned(), metric.kind().into());
            }

            if !set_type {
                map.insert("type".to_owned(), metric.value().clone().into());
            }

            if !set_namespace {
                if let Some(namespace) = metric.namespace() {
                    map.insert("namespace".to_owned(), namespace.to_owned().into());
                }
            }

            if !set_timestamp {
                if let Some(timestamp) = metric.timestamp() {
                    map.insert("timestamp".to_owned(), timestamp.into());
                }
            }

            if !set_tags {
                if let Some(tags) = metric.tags().cloned() {
                    map.insert(
                        "tags".to_owned(),
                        tags.into_iter()
                            .map(|(tag, value)| (tag, value.into()))
                            .collect::<BTreeMap<_, _>>()
                            .into(),
                    );
                }
            }

            break;
        }

        // For non-root paths, we contiuously populate the value with the
        // relevant data.
        if let Some(SegmentBuf::Field(field)) = path.iter().next() {
            match field.as_str() {
                "name" if !set_name => {
                    set_name = true;
                    map.insert("name".to_owned(), metric.name().to_owned().into());
                }
                "kind" if !set_kind => {
                    set_kind = true;
                    map.insert("kind".to_owned(), metric.kind().into());
                }
                "type" if !set_type => {
                    set_type = true;
                    map.insert("type".to_owned(), metric.value().clone().into());
                }
                "namespace" if !set_namespace && metric.namespace().is_some() => {
                    set_namespace = true;
                    map.insert(
                        "namespace".to_owned(),
                        metric.namespace().unwrap().to_owned().into(),
                    );
                }
                "timestamp" if !set_timestamp && metric.timestamp().is_some() => {
                    set_timestamp = true;
                    map.insert("timestamp".to_owned(), metric.timestamp().unwrap().into());
                }
                "tags" if !set_tags && metric.tags().is_some() => {
                    set_tags = true;
                    map.insert(
                        "tags".to_owned(),
                        metric
                            .tags()
                            .cloned()
                            .unwrap()
                            .into_iter()
                            .map(|(tag, value)| (tag, value.into()))
                            .collect::<BTreeMap<_, _>>()
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
    use pretty_assertions::assert_eq;
    use vector_common::btreemap;
    use vrl_lib::Target;

    use super::{
        super::{metric::MetricTags, MetricValue},
        *,
    };

    #[test]
    fn log_get() {
        use lookup::{FieldBuf, SegmentBuf};
        use vector_common::btreemap;

        let cases = vec![
            (BTreeMap::new(), vec![], Ok(Some(BTreeMap::new().into()))),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![],
                Ok(Some(BTreeMap::from([("foo".into(), "bar".into())]).into())),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![SegmentBuf::from("foo")],
                Ok(Some("bar".into())),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![SegmentBuf::from("bar")],
                Ok(None),
            ),
            (
                btreemap! { "foo" => vec![btreemap! { "bar" => true }] },
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(0),
                    SegmentBuf::from("bar"),
                ],
                Ok(Some(true.into())),
            ),
            (
                btreemap! { "foo" => btreemap! { "bar baz" => btreemap! { "baz" => 2 } } },
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(vec![FieldBuf::from("qux"), FieldBuf::from(r#""bar baz""#)]),
                    SegmentBuf::from("baz"),
                ],
                Ok(Some(2.into())),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: BTreeMap<String, Value> = value;
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let target = VrlTarget::new(Event::Log(LogEvent::from(value)), &info);
            let path = LookupBuf::from_segments(segments);

            assert_eq!(
                vrl_lib::Target::target_get(&target, &path).map(Option::<&Value>::cloned),
                expect
            );
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn log_insert() {
        use lookup::SegmentBuf;
        use vector_common::btreemap;

        let cases = vec![
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![],
                btreemap! { "baz" => "qux" }.into(),
                btreemap! { "baz" => "qux" },
                Ok(()),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![SegmentBuf::from("foo")],
                "baz".into(),
                btreemap! { "foo" => "baz" },
                Ok(()),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(2),
                    SegmentBuf::from("bar baz"),
                    SegmentBuf::from("a"),
                    SegmentBuf::from("b"),
                ],
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
                vec![SegmentBuf::from("foo"), SegmentBuf::from(5)],
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
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![]) },
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into()]) },
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                btreemap! { "foo" => vec!["baz"] },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                "baz".into(),
                btreemap! { "foo" => Value::Array(vec!["baz".into(), 1.into()]) },
                Ok(()),
            ),
            (
                btreemap! { "foo" => Value::Array(vec![0.into(), 1.into()]) },
                vec![SegmentBuf::from("foo"), SegmentBuf::from(1)],
                "baz".into(),
                btreemap! { "foo" => Value::Array(vec![0.into(), "baz".into()]) },
                Ok(()),
            ),
        ];

        for (object, segments, value, expect, result) in cases {
            let object: BTreeMap<String, Value> = object;
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(Event::Log(LogEvent::from(object)), &info);
            let expect = LogEvent::from(expect);
            let value: ::value::Value = value;
            let path = LookupBuf::from_segments(segments);

            assert_eq!(
                vrl_lib::Target::target_insert(&mut target, &path, value.clone()),
                result
            );
            assert_eq!(
                vrl_lib::Target::target_get(&target, &path).map(Option::<&Value>::cloned),
                Ok(Some(value))
            );
            assert_eq!(
                match target.into_events() {
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
        use lookup::{FieldBuf, SegmentBuf};
        use vector_common::btreemap;

        let cases = vec![
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![SegmentBuf::from("foo")],
                false,
                Some(BTreeMap::new().into()),
            ),
            (
                BTreeMap::from([("foo".into(), "bar".into())]),
                vec![SegmentBuf::from(vec![
                    FieldBuf::from(r#""foo bar""#),
                    FieldBuf::from("foo"),
                ])],
                false,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                vec![],
                false,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                vec![],
                true,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! { "foo" => vec![0] },
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                false,
                Some(btreemap! { "foo" => Value::Array(vec![]) }.into()),
            ),
            (
                btreemap! { "foo" => vec![0] },
                vec![SegmentBuf::from("foo"), SegmentBuf::from(0)],
                true,
                Some(BTreeMap::new().into()),
            ),
            (
                btreemap! {
                    "foo" => btreemap! { "bar baz" => vec![0] },
                    "bar" => "baz",
                },
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
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
                vec![
                    SegmentBuf::from("foo"),
                    SegmentBuf::from(r#""bar baz""#),
                    SegmentBuf::from(0),
                ],
                true,
                Some(btreemap! { "bar" => "baz" }.into()),
            ),
        ];

        for (object, segments, compact, expect) in cases {
            let info = ProgramInfo {
                fallible: false,
                abortable: false,
                target_queries: vec![],
                target_assignments: vec![],
            };
            let mut target = VrlTarget::new(Event::Log(LogEvent::from(object)), &info);
            let path = LookupBuf::from_segments(segments);
            let removed = vrl_lib::Target::target_get(&target, &path)
                .unwrap()
                .cloned();

            assert_eq!(
                vrl_lib::Target::target_remove(&mut target, &path, compact),
                Ok(removed)
            );
            assert_eq!(
                vrl_lib::Target::target_get(&target, &LookupBuf::root())
                    .map(Option::<&Value>::cloned),
                Ok(expect)
            );
        }
    }

    #[test]
    fn log_into_events() {
        use vector_common::btreemap;

        let cases = vec![
            (
                ::value::Value::from(btreemap! {"foo" => "bar"}),
                vec![btreemap! {"foo" => "bar"}],
            ),
            (::value::Value::from(1), vec![btreemap! {"message" => 1}]),
            (
                ::value::Value::from("2"),
                vec![btreemap! {"message" => "2"}],
            ),
            (
                ::value::Value::from(true),
                vec![btreemap! {"message" => true}],
            ),
            (
                ::value::Value::from(vec![
                    ::value::Value::from(1),
                    ::value::Value::from("2"),
                    ::value::Value::from(true),
                    ::value::Value::from(btreemap! {"foo" => "bar"}),
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
            );

            ::vrl_lib::Target::target_insert(&mut target, &LookupBuf::root(), value).unwrap();

            assert_eq!(
                match target.into_events() {
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
        .with_tags(Some({
            let mut map = MetricTags::new();
            map.insert("tig".to_string(), "tog".to_string());
            map
        }))
        .with_timestamp(Some(Utc.ymd(2020, 12, 10).and_hms(12, 0, 0)));

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![
                "name".into(),
                "namespace".into(),
                "timestamp".into(),
                "kind".into(),
                "type".into(),
                "tags".into(),
            ],
            target_assignments: vec![],
        };
        let target = VrlTarget::new(Event::Metric(metric), &info);

        assert_eq!(
            Ok(Some(
                btreemap! {
                    "name" => "zub",
                    "namespace" => "zoob",
                    "timestamp" => Utc.ymd(2020, 12, 10).and_hms(12, 0, 0),
                    "tags" => btreemap! { "tig" => "tog" },
                    "kind" => "absolute",
                    "type" => "counter",
                }
                .into()
            )),
            target
                .target_get(&LookupBuf::root())
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
        .with_tags(Some({
            let mut map = MetricTags::new();
            map.insert("tig".to_string(), "tog".to_string());
            map
        }));

        let cases = vec![
            (
                "name",                             // Path
                Some(::value::Value::from("name")), // Current value
                ::value::Value::from("namefoo"),    // New value
                false,                              // Test deletion
            ),
            ("namespace", None, "namespacefoo".into(), true),
            (
                "timestamp",
                None,
                Utc.ymd(2020, 12, 8).and_hms(12, 0, 0).into(),
                true,
            ),
            (
                "kind",
                Some(::value::Value::from("absolute")),
                "incremental".into(),
                false,
            ),
            ("tags.thing", None, "footag".into(), true),
        ];

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![
                "name".into(),
                "namespace".into(),
                "timestamp".into(),
                "kind".into(),
            ],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info);

        for (path, current, new, delete) in cases {
            let path = LookupBuf::from_str(path).unwrap();

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
    fn metric_invalid_paths() {
        let metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        );

        let validpaths_get = vec![
            ".name",
            ".namespace",
            ".timestamp",
            ".kind",
            ".tags",
            ".type",
        ];

        let validpaths_set = vec![".name", ".namespace", ".timestamp", ".kind", ".tags"];

        let info = ProgramInfo {
            fallible: false,
            abortable: false,
            target_queries: vec![],
            target_assignments: vec![],
        };
        let mut target = VrlTarget::new(Event::Metric(metric), &info);

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_get.join(", ")
            )),
            target.target_get(&LookupBuf::from_str("zork").unwrap())
        );

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            target.target_insert(&LookupBuf::from_str("zork").unwrap(), "thing".into())
        );

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            target.target_remove(&LookupBuf::from_str("zork").unwrap(), true)
        );

        assert_eq!(
            Err(format!(
                "invalid path tags.foo.flork: expected one of {}",
                validpaths_get.join(", ")
            )),
            target.target_get(&LookupBuf::from_str("tags.foo.flork").unwrap())
        );
    }
}
