use std::{collections::BTreeMap, convert::TryFrom, sync::Arc};

use lookup::LookupBuf;
use snafu::Snafu;

use super::{Event, EventMetadata, LogEvent, Metric, MetricKind, Value};
use crate::config::log_schema;

const VALID_METRIC_PATHS_SET: &str = ".name, .namespace, .timestamp, .kind, .tags";

/// We can get the `type` of the metric in Remap, but can't set it.
const VALID_METRIC_PATHS_GET: &str = ".name, .namespace, .timestamp, .kind, .tags, .type";

/// Metrics aren't interested in paths that have a length longer than 3.
///
/// The longest path is 2, and we need to check that a third segment doesn't exist as we don't want
/// fields such as `.tags.host.thing`.
const MAX_METRIC_PATH_DEPTH: usize = 3;

/// An adapter to turn `Event`s into `vrl_core::Target`s.
#[derive(Debug, Clone)]
pub enum VrlTarget {
    // `LogEvent` is essentially just a destructured `event::LogEvent`, but without the semantics
    // that `fields` must always be a `Map` variant.
    LogEvent(Value, EventMetadata),
    Metric(Metric),
}

impl VrlTarget {
    pub fn new(event: Event) -> Self {
        match event {
            Event::Log(event) => {
                let (fields, metadata) = event.into_parts();
                VrlTarget::LogEvent(Value::Map(fields), metadata)
            }
            Event::Metric(event) => VrlTarget::Metric(event),
        }
    }

    /// Turn the target back into events.
    ///
    /// This returns an iterator of events as one event can be turned into multiple by assigning an
    /// array to `.` in VRL.
    pub fn into_events(self) -> impl Iterator<Item = Event> {
        match self {
            VrlTarget::LogEvent(value, metadata) => {
                Box::new(value_into_log_events(value, metadata)) as Box<dyn Iterator<Item = Event>>
            }
            VrlTarget::Metric(metric) => {
                Box::new(std::iter::once(Event::Metric(metric))) as Box<dyn Iterator<Item = Event>>
            }
        }
    }
}

impl vrl_core::Target for VrlTarget {
    fn insert(&mut self, path: &LookupBuf, value: vrl_core::Value) -> Result<(), String> {
        match self {
            VrlTarget::LogEvent(ref mut log, _) => log
                .insert(path.clone(), value)
                .map(|_| ())
                .map_err(|err| err.to_string()),
            VrlTarget::Metric(ref mut metric) => {
                if path.is_root() {
                    return Err(MetricPathError::SetPathError.to_string());
                }

                if let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH).get(0) {
                    match paths.as_slice() {
                        ["tags"] => {
                            let value = value.try_object().map_err(|e| e.to_string())?;
                            for (field, value) in &value {
                                metric.insert_tag(
                                    field.as_str().to_owned(),
                                    value
                                        .try_bytes_utf8_lossy()
                                        .map_err(|e| e.to_string())?
                                        .into_owned(),
                                );
                            }
                            return Ok(());
                        }
                        ["tags", field] => {
                            let value = value.try_bytes().map_err(|e| e.to_string())?;
                            metric.insert_tag(
                                (*field).to_owned(),
                                String::from_utf8_lossy(&value).into_owned(),
                            );
                            return Ok(());
                        }
                        ["name"] => {
                            let value = value.try_bytes().map_err(|e| e.to_string())?;
                            metric.series.name.name = String::from_utf8_lossy(&value).into_owned();
                            return Ok(());
                        }
                        ["namespace"] => {
                            let value = value.try_bytes().map_err(|e| e.to_string())?;
                            metric.series.name.namespace =
                                Some(String::from_utf8_lossy(&value).into_owned());
                            return Ok(());
                        }
                        ["timestamp"] => {
                            let value = value.try_timestamp().map_err(|e| e.to_string())?;
                            metric.data.timestamp = Some(value);
                            return Ok(());
                        }
                        ["kind"] => {
                            metric.data.kind = MetricKind::try_from(value)?;
                            return Ok(());
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

                Err(MetricPathError::InvalidPath {
                    path: &path.to_string(),
                    expected: VALID_METRIC_PATHS_SET,
                }
                .to_string())
            }
        }
    }

    fn get(&self, path: &LookupBuf) -> std::result::Result<Option<vrl_core::Value>, String> {
        match self {
            VrlTarget::LogEvent(log, _) => log
                .get(path)
                .map(|val| val.map(|val| val.clone().into()))
                .map_err(|err| err.to_string()),
            VrlTarget::Metric(metric) => {
                if path.is_root() {
                    let mut map = BTreeMap::<String, vrl_core::Value>::new();
                    map.insert("name".to_string(), metric.series.name.name.clone().into());
                    if let Some(ref namespace) = metric.series.name.namespace {
                        map.insert("namespace".to_string(), namespace.clone().into());
                    }
                    if let Some(timestamp) = metric.data.timestamp {
                        map.insert("timestamp".to_string(), timestamp.into());
                    }
                    map.insert("kind".to_string(), metric.data.kind.into());
                    if let Some(tags) = metric.tags() {
                        map.insert(
                            "tags".to_string(),
                            tags.iter()
                                .map(|(tag, value)| (tag.clone(), value.clone().into()))
                                .collect::<BTreeMap<_, _>>()
                                .into(),
                        );
                    }
                    map.insert("type".to_string(), metric.data.value.clone().into());

                    return Ok(Some(map.into()));
                }

                for paths in path.to_alternative_components(MAX_METRIC_PATH_DEPTH) {
                    match paths.as_slice() {
                        ["name"] => return Ok(Some(metric.name().to_string().into())),
                        ["namespace"] => match &metric.series.name.namespace {
                            Some(namespace) => return Ok(Some(namespace.clone().into())),
                            None => continue,
                        },
                        ["timestamp"] => match metric.data.timestamp {
                            Some(timestamp) => return Ok(Some(timestamp.into())),
                            None => continue,
                        },
                        ["kind"] => return Ok(Some(metric.data.kind.into())),
                        ["tags"] => {
                            return Ok(metric.tags().map(|map| {
                                map.iter()
                                    .map(|(k, v)| (k.clone(), v.clone().into()))
                                    .collect::<vrl_core::Value>()
                            }))
                        }
                        ["tags", field] => match metric.tag_value(field) {
                            Some(value) => return Ok(Some(value.into())),
                            None => continue,
                        },
                        ["type"] => return Ok(Some(metric.data.value.clone().into())),
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
        }
    }

    fn remove(
        &mut self,
        path: &LookupBuf,
        compact: bool,
    ) -> Result<Option<vrl_core::Value>, String> {
        match self {
            VrlTarget::LogEvent(ref mut log, _) => {
                if path.is_root() {
                    Ok(Some({
                        let mut map = Value::Map(BTreeMap::new());
                        std::mem::swap(log, &mut map);
                        map.into()
                    }))
                } else {
                    log.remove(path, compact)
                        .map(|val| val.map(Into::into))
                        .map_err(|err| err.to_string())
                }
            }
            VrlTarget::Metric(ref mut metric) => {
                if path.is_root() {
                    return Err(MetricPathError::SetPathError.to_string());
                }

                if let Some(paths) = path.to_alternative_components(MAX_METRIC_PATH_DEPTH).get(0) {
                    match paths.as_slice() {
                        ["namespace"] => {
                            return Ok(metric.series.name.namespace.take().map(Into::into))
                        }
                        ["timestamp"] => return Ok(metric.data.timestamp.take().map(Into::into)),
                        ["tags"] => {
                            return Ok(metric.series.tags.take().map(|map| {
                                map.into_iter()
                                    .map(|(k, v)| (k, v.into()))
                                    .collect::<vrl_core::Value>()
                            }))
                        }
                        ["tags", field] => return Ok(metric.remove_tag(field).map(Into::into)),
                        _ => {
                            return Err(MetricPathError::InvalidPath {
                                path: &path.to_string(),
                                expected: VALID_METRIC_PATHS_SET,
                            }
                            .to_string())
                        }
                    }
                }

                Ok(None)
            }
        }
    }

    fn get_metadata(&self, key: &str) -> Result<Option<vrl_core::Value>, String> {
        let metadata = match self {
            VrlTarget::LogEvent(_, metadata) => metadata,
            VrlTarget::Metric(metric) => metric.metadata(),
        };

        match key {
            "datadog_api_key" => Ok(metadata
                .datadog_api_key()
                .as_ref()
                .map(|api_key| vrl_core::Value::from(api_key.to_string()))),
            "splunk_hec_token" => Ok(metadata
                .splunk_hec_token()
                .as_ref()
                .map(|token| vrl_core::Value::from(token.to_string()))),
            _ => Err(format!("key {} not available", key)),
        }
    }

    fn set_metadata(&mut self, key: &str, value: String) -> Result<(), String> {
        let metadata = match self {
            VrlTarget::LogEvent(_, metadata) => metadata,
            VrlTarget::Metric(metric) => metric.metadata_mut(),
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
            VrlTarget::LogEvent(_, metadata) => metadata,
            VrlTarget::Metric(metric) => metric.metadata_mut(),
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

impl From<Event> for VrlTarget {
    fn from(event: Event) -> Self {
        VrlTarget::new(event)
    }
}

// Turn a `Value` back into `LogEvents`:
// * In the common case, where `.` is a map, just create an event using it as the event fields.
// * If `.` is an array, map over all of the values to create log events:
//   * If an element is an object, create an event using that as fields.
//   * If an element is anything else, assign to the `message` key.
// * If `.` is anything else, assign to the `message` key.
fn value_into_log_events(value: Value, metadata: EventMetadata) -> impl Iterator<Item = Event> {
    match value {
        Value::Map(object) => Box::new(std::iter::once(Event::from(LogEvent::from_parts(
            object, metadata,
        )))) as Box<dyn Iterator<Item = Event>>,
        Value::Array(values) => Box::new(values.into_iter().map(move |v| match v {
            Value::Map(object) => Event::from(LogEvent::from_parts(object, metadata.clone())),
            v => {
                let mut log = LogEvent::new_with_metadata(metadata.clone());
                log.insert(log_schema().message_key(), v);
                Event::from(log)
            }
        })) as Box<dyn Iterator<Item = Event>>,
        v => {
            let mut log = LogEvent::new_with_metadata(metadata);
            log.insert(log_schema().message_key(), v);
            Box::new(std::iter::once(Event::from(log))) as Box<dyn Iterator<Item = Event>>
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
    use vrl_core::{self, Target};

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
            let target = VrlTarget::new(Event::Log(LogEvent::from(value)));
            let path = LookupBuf::from_segments(segments);

            assert_eq!(vrl_core::Target::get(&target, &path), expect);
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
            let mut target = VrlTarget::new(Event::Log(LogEvent::from(object)));
            let expect = LogEvent::from(expect);
            let value: vrl_core::Value = value;
            let path = LookupBuf::from_segments(segments);

            assert_eq!(
                vrl_core::Target::insert(&mut target, &path, value.clone()),
                result
            );
            assert_eq!(vrl_core::Target::get(&target, &path), Ok(Some(value)));
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
            let mut target = VrlTarget::new(Event::Log(LogEvent::from(object)));
            let path = LookupBuf::from_segments(segments);
            let removed = vrl_core::Target::get(&target, &path).unwrap();

            assert_eq!(
                vrl_core::Target::remove(&mut target, &path, compact),
                Ok(removed)
            );
            assert_eq!(
                vrl_core::Target::get(&target, &LookupBuf::root()),
                Ok(expect)
            );
        }
    }

    #[test]
    fn log_into_events() {
        use vector_common::btreemap;

        let cases = vec![
            (
                vrl_core::Value::from(btreemap! {"foo" => "bar"}),
                vec![btreemap! {"foo" => "bar"}],
            ),
            (vrl_core::Value::from(1), vec![btreemap! {"message" => 1}]),
            (
                vrl_core::Value::from("2"),
                vec![btreemap! {"message" => "2"}],
            ),
            (
                vrl_core::Value::from(true),
                vec![btreemap! {"message" => true}],
            ),
            (
                vrl_core::Value::from(vec![
                    vrl_core::Value::from(1),
                    vrl_core::Value::from("2"),
                    vrl_core::Value::from(true),
                    vrl_core::Value::from(btreemap! {"foo" => "bar"}),
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
            let mut target =
                VrlTarget::new(Event::Log(LogEvent::new_with_metadata(metadata.clone())));

            vrl_core::Target::insert(&mut target, &LookupBuf::root(), value).unwrap();

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

        let target = VrlTarget::new(Event::Metric(metric));

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
            target.get(&LookupBuf::root())
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
                "name",                              // Path
                Some(vrl_core::Value::from("name")), // Current value
                vrl_core::Value::from("namefoo"),    // New value
                false,                               // Test deletion
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
                Some(vrl_core::Value::from("absolute")),
                "incremental".into(),
                false,
            ),
            ("tags.thing", None, "footag".into(), true),
        ];

        let mut target = VrlTarget::new(Event::Metric(metric));

        for (path, current, new, delete) in cases {
            let path = LookupBuf::from_str(path).unwrap();

            assert_eq!(Ok(current), target.get(&path));
            assert_eq!(Ok(()), target.insert(&path, new.clone()));
            assert_eq!(Ok(Some(new.clone())), target.get(&path));

            if delete {
                assert_eq!(Ok(Some(new)), target.remove(&path, true));
                assert_eq!(Ok(None), target.get(&path));
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

        let mut target = VrlTarget::new(Event::Metric(metric));

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_get.join(", ")
            )),
            target.get(&LookupBuf::from_str("zork").unwrap())
        );

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            target.insert(&LookupBuf::from_str("zork").unwrap(), "thing".into())
        );

        assert_eq!(
            Err(format!(
                "invalid path zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            target.remove(&LookupBuf::from_str("zork").unwrap(), true)
        );

        assert_eq!(
            Err(format!(
                "invalid path tags.foo.flork: expected one of {}",
                validpaths_get.join(", ")
            )),
            target.get(&LookupBuf::from_str("tags.foo.flork").unwrap())
        );
    }
}
