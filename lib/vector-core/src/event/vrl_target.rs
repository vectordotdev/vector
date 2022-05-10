use std::{collections::BTreeMap, convert::TryFrom, sync::Arc};

use lookup::{LookupBuf, SegmentBuf};
use snafu::Snafu;
use vrl_lib::{prelude::VrlValueConvert, ProgramInfo};

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

impl VrlTarget {
    pub fn new(event: Event, info: &ProgramInfo) -> Self {
        match event {
            Event::Log(event) => {
                let (fields, metadata) = event.into_parts();
                VrlTarget::LogEvent(Value::Object(fields), metadata)
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
    pub fn into_events(self) -> impl Iterator<Item = Event> {
        match self {
            VrlTarget::LogEvent(value, metadata) => {
                Box::new(value_into_logevents(value, metadata).map(Event::Log))
                    as Box<dyn Iterator<Item = Event>>
            }
            VrlTarget::Metric { metric, .. } => {
                Box::new(std::iter::once(Event::Metric(metric))) as Box<dyn Iterator<Item = Event>>
            }
            VrlTarget::Trace(value, metadata) => Box::new(
                value_into_logevents(value, metadata)
                    .map(|log| Event::Trace(TraceEvent::from(log))),
            ) as Box<dyn Iterator<Item = Event>>,
        }
    }
}

impl vrl_lib::Target for VrlTarget {
    fn target_insert(&mut self, path: &LookupBuf, value: ::value::Value) -> Result<(), String> {
        match self {
            VrlTarget::LogEvent(ref mut log, _) | VrlTarget::Trace(ref mut log, _) => log
                .insert(path.clone(), value)
                .map(|_| ())
                .map_err(|err| err.to_string()),
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
            VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => {
                log.get(path).map_err(|err| err.to_string())
            }
            VrlTarget::Metric { value, .. } => target_get_metric(path, value),
        }
    }

    fn target_get_mut(&mut self, path: &LookupBuf) -> Result<Option<&mut Value>, String> {
        match self {
            VrlTarget::LogEvent(log, _) | VrlTarget::Trace(log, _) => {
                log.get_mut(path).map_err(|err| err.to_string())
            }
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
                if path.is_root() {
                    Ok(Some({
                        let mut map = Value::Object(BTreeMap::new());
                        std::mem::swap(log, &mut map);
                        map
                    }))
                } else {
                    log.remove(path, compact)
                        .map(|val| val.map(Into::into))
                        .map_err(|err| err.to_string())
                }
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

    fn get_metadata(&self, key: &str) -> Result<Option<::value::Value>, String> {
        let metadata = match self {
            VrlTarget::LogEvent(_, metadata) | VrlTarget::Trace(_, metadata) => metadata,
            VrlTarget::Metric { metric, .. } => metric.metadata(),
        };

        match key {
            "datadog_api_key" => Ok(metadata
                .datadog_api_key()
                .as_ref()
                .map(|api_key| ::value::Value::from(api_key.to_string()))),
            "splunk_hec_token" => Ok(metadata
                .splunk_hec_token()
                .as_ref()
                .map(|token| ::value::Value::from(token.to_string()))),
            _ => Err(format!("key {} not available", key)),
        }
    }

    fn set_metadata(&mut self, key: &str, value: String) -> Result<(), String> {
        let metadata = match self {
            VrlTarget::LogEvent(_, metadata) | VrlTarget::Trace(_, metadata) => metadata,
            VrlTarget::Metric { metric, .. } => metric.metadata_mut(),
        };

        match key {
            "datadog_api_key" => {
                metadata.set_datadog_api_key(Some(Arc::from(value.as_str())));
                Ok(())
            }
            "splunk_hec_token" => {
                metadata.set_splunk_hec_token(Some(Arc::from(value.as_str())));
                Ok(())
            }
            _ => Err(format!("key {} not available", key)),
        }
    }

    fn remove_metadata(&mut self, key: &str) -> Result<(), String> {
        let metadata = match self {
            VrlTarget::LogEvent(_, metadata) | VrlTarget::Trace(_, metadata) => metadata,
            VrlTarget::Metric { metric, .. } => metric.metadata_mut(),
        };

        match key {
            "datadog_api_key" => {
                metadata.set_datadog_api_key(None);
                Ok(())
            }
            "splunk_hec_token" => {
                metadata.set_splunk_hec_token(None);
                Ok(())
            }
            _ => Err(format!("key {} not available", key)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum VrlImmutableTarget<'a> {
    LogEvent(&'a LogEvent),
    Metric { metric: &'a Metric, value: Value },
    Trace(&'a TraceEvent),
}

impl<'a> VrlImmutableTarget<'a> {
    pub fn new(event: &'a Event, info: &ProgramInfo) -> Self {
        match event {
            Event::Log(event) => VrlImmutableTarget::LogEvent(event),
            Event::Metric(metric) => {
                // We pre-generate [`Value`] types for the metric fields accessed in
                // the event. This allows us to then return references to those
                // values, even if the field is accessed more than once.
                let value = precompute_metric_value(metric, info);

                VrlImmutableTarget::Metric { metric, value }
            }
            Event::Trace(event) => VrlImmutableTarget::Trace(event),
        }
    }
}

impl<'a> vrl_lib::Target for VrlImmutableTarget<'a> {
    fn target_insert(&mut self, _path: &LookupBuf, _value: ::value::Value) -> Result<(), String> {
        Err("cannot modify immutable target".to_string())
    }

    #[allow(clippy::redundant_closure_for_method_calls)] // false positive
    fn target_get(&self, path: &LookupBuf) -> Result<Option<&Value>, String> {
        match self {
            VrlImmutableTarget::LogEvent(log) => Ok(log.lookup(path)),
            VrlImmutableTarget::Trace(log) => Ok(log.lookup(path)),
            VrlImmutableTarget::Metric { value, .. } => target_get_metric(path, value),
        }
    }

    fn target_get_mut(&mut self, _path: &LookupBuf) -> Result<Option<&mut Value>, String> {
        Err("cannot modify immutable target".to_string())
    }

    fn target_remove(
        &mut self,
        _path: &LookupBuf,
        _compact: bool,
    ) -> Result<Option<::value::Value>, String> {
        Err("cannot modify immutable target".to_string())
    }

    fn get_metadata(&self, key: &str) -> Result<Option<::value::Value>, String> {
        let metadata = match self {
            VrlImmutableTarget::LogEvent(event) => event.metadata(),
            VrlImmutableTarget::Trace(event) => event.metadata(),
            VrlImmutableTarget::Metric { metric, .. } => metric.metadata(),
        };

        match key {
            "datadog_api_key" => Ok(metadata
                .datadog_api_key()
                .as_ref()
                .map(|api_key| ::value::Value::from(api_key.to_string()))),
            "splunk_hec_token" => Ok(metadata
                .splunk_hec_token()
                .as_ref()
                .map(|token| ::value::Value::from(token.to_string()))),
            _ => Err(format!("key {} not available", key)),
        }
    }

    fn set_metadata(&mut self, _key: &str, _value: String) -> Result<(), String> {
        Err("cannot modify immutable target".to_string())
    }

    fn remove_metadata(&mut self, _key: &str) -> Result<(), String> {
        Err("cannot modify immutable target".to_string())
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

// Turn a `Value` back into `LogEvents`:
// * In the common case, where `.` is a map, just create an event using it as the event fields.
// * If `.` is an array, map over all of the values to create log events:
//   * If an element is an object, create an event using that as fields.
//   * If an element is anything else, assign to the `message` key.
// * If `.` is anything else, assign to the `message` key.
fn value_into_logevents(value: Value, metadata: EventMetadata) -> impl Iterator<Item = LogEvent> {
    match value {
        Value::Object(object) => Box::new(std::iter::once(LogEvent::from_parts(object, metadata)))
            as Box<dyn Iterator<Item = LogEvent>>,
        Value::Array(values) => Box::new(values.into_iter().map(move |v| match v {
            Value::Object(object) => LogEvent::from_parts(object, metadata.clone()),
            v => {
                let mut log = LogEvent::new_with_metadata(metadata.clone());
                log.insert(log_schema().message_key(), v);
                log
            }
        })) as Box<dyn Iterator<Item = LogEvent>>,
        v => {
            let mut log = LogEvent::new_with_metadata(metadata);
            log.insert(log_schema().message_key(), v);
            Box::new(std::iter::once(log)) as Box<dyn Iterator<Item = LogEvent>>
        }
    }
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
            (btreemap! {}, vec![], Ok(Some(btreemap! {}.into()))),
            (
                btreemap! { "foo" => "bar" },
                vec![],
                Ok(Some(btreemap! { "foo" => "bar" }.into())),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![SegmentBuf::from("foo")],
                Ok(Some("bar".into())),
            ),
            (
                btreemap! { "foo" => "bar" },
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
                btreemap! { "foo" => "bar" },
                vec![],
                btreemap! { "baz" => "qux" }.into(),
                btreemap! { "baz" => "qux" },
                Ok(()),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![SegmentBuf::from("foo")],
                "baz".into(),
                btreemap! { "foo" => "baz" },
                Ok(()),
            ),
            (
                btreemap! { "foo" => "bar" },
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
                btreemap! { "foo" => "bar" },
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
            assert_eq!(target.into_events().next().unwrap(), Event::Log(expect));
        }
    }

    #[test]
    fn log_remove() {
        use lookup::{FieldBuf, SegmentBuf};
        use vector_common::btreemap;

        let cases = vec![
            (
                btreemap! { "foo" => "bar" },
                vec![SegmentBuf::from("foo")],
                false,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => "bar" },
                vec![SegmentBuf::from(vec![
                    FieldBuf::from(r#""foo bar""#),
                    FieldBuf::from("foo"),
                ])],
                false,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                vec![],
                false,
                Some(btreemap! {}.into()),
            ),
            (
                btreemap! { "foo" => "bar", "baz" => "qux" },
                vec![],
                true,
                Some(btreemap! {}.into()),
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
                Some(btreemap! {}.into()),
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
                target.into_events().collect::<Vec<_>>(),
                expect
                    .into_iter()
                    .map(|v| Event::Log(LogEvent::from_parts(v, metadata.clone())))
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
