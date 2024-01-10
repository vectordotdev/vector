//! Functionality for managing template fields used by Vector's sinks.
use std::{borrow::Cow, convert::TryFrom, fmt, hash::Hash, path::PathBuf};

use bytes::Bytes;
use chrono::{
    format::{strftime::StrftimeItems, Item},
    FixedOffset, Utc,
};
use once_cell::sync::Lazy;
use regex::Regex;
use snafu::Snafu;
use vector_lib::configurable::{configurable_component, ConfigurableString};
use vector_lib::lookup::lookup_v2::parse_target_path;

use crate::{
    config::log_schema,
    event::{EventRef, Metric, Value},
};

static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap());

/// Errors raised whilst parsing a Template field.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum TemplateParseError {
    #[snafu(display("Invalid strftime item"))]
    StrftimeError,
    #[snafu(display("Invalid field path in template {:?} (see https://vector.dev/docs/reference/configuration/template-syntax/)", path))]
    InvalidPathSyntax { path: String },
}

/// Errors raised whilst rendering a Template.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum TemplateRenderingError {
    #[snafu(display("Missing fields on event: {:?}", missing_keys))]
    MissingKeys { missing_keys: Vec<String> },
}

/// A templated field.
///
/// In many cases, components can be configured so that part of the component's functionality can be
/// customized on a per-event basis. For example, you have a sink that writes events to a file and you want to
/// specify which file an event should go to by using an event field as part of the
/// input to the filename used.
///
/// By using `Template`, users can specify either fixed strings or templated strings. Templated strings use a common syntax to
/// refer to fields in an event that is used as the input data when rendering the template. An example of a fixed string
/// is `my-file.log`. An example of a template string is `my-file-{{key}}.log`, where `{{key}}`
/// is the key's value when the template is rendered into a string.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct Template {
    src: String,

    #[serde(skip)]
    parts: Vec<Part>,

    #[serde(skip)]
    is_static: bool,

    #[serde(skip)]
    reserve_size: usize,

    #[serde(skip)]
    tz_offset: Option<FixedOffset>,
}

impl TryFrom<&str> for Template {
    type Error = TemplateParseError;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        Template::try_from(Cow::Borrowed(src))
    }
}

impl TryFrom<String> for Template {
    type Error = TemplateParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        Template::try_from(Cow::Owned(src))
    }
}

impl TryFrom<PathBuf> for Template {
    type Error = TemplateParseError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        Template::try_from(p.to_string_lossy().into_owned())
    }
}

impl TryFrom<Cow<'_, str>> for Template {
    type Error = TemplateParseError;

    fn try_from(src: Cow<'_, str>) -> Result<Self, Self::Error> {
        parse_template(&src).map(|parts| {
            let is_static =
                parts.is_empty() || (parts.len() == 1 && matches!(parts[0], Part::Literal(..)));

            // Calculate a minimum size to reserve for rendered string. This doesn't have to be
            // exact, and can't be because of references and time format specifiers. We just want a
            // better starting number than 0 to avoid the first reallocations if possible.
            let reserve_size = parts
                .iter()
                .map(|part| match part {
                    Part::Literal(lit) => lit.len(),
                    // We can't really put a useful number here, assume at least one byte will come
                    // from the input event.
                    Part::Reference(_path) => 1,
                    Part::Strftime(parsed) => parsed.reserve_size(),
                })
                .sum();

            Template {
                parts,
                src: src.into_owned(),
                is_static,
                reserve_size,
                tz_offset: None,
            }
        })
    }
}

impl From<Template> for String {
    fn from(template: Template) -> String {
        template.src
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.src)
    }
}

// This is safe because we literally defer to `String` for the schema of `Template`.
impl ConfigurableString for Template {}

impl Template {
    /// set tz offset
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.tz_offset = tz_offset;
        self
    }
    /// Renders the given template with data from the event.
    pub fn render<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<Bytes, TemplateRenderingError> {
        self.render_string(event.into()).map(Into::into)
    }

    /// Renders the given template with data from the event.
    pub fn render_string<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<String, TemplateRenderingError> {
        if self.is_static {
            Ok(self.src.clone())
        } else {
            self.render_event(event.into())
        }
    }

    fn render_event(&self, event: EventRef<'_>) -> Result<String, TemplateRenderingError> {
        let mut missing_keys = Vec::new();
        let mut out = String::with_capacity(self.reserve_size);
        for part in &self.parts {
            match part {
                Part::Literal(lit) => out.push_str(lit),
                Part::Strftime(items) => {
                    out.push_str(&render_timestamp(items, event, self.tz_offset))
                }
                Part::Reference(key) => {
                    out.push_str(
                        &match event {
                            EventRef::Log(log) => log
                                .parse_path_and_get_value(key)
                                .ok()
                                .and_then(|v| v.map(Value::to_string_lossy)),
                            EventRef::Metric(metric) => {
                                render_metric_field(key, metric).map(Cow::Borrowed)
                            }
                            EventRef::Trace(trace) => trace
                                .parse_path_and_get_value(key)
                                .ok()
                                .and_then(|v| v.map(Value::to_string_lossy)),
                        }
                        .unwrap_or_else(|| {
                            missing_keys.push(key.to_owned());
                            Cow::Borrowed("")
                        }),
                    );
                }
            }
        }
        if missing_keys.is_empty() {
            Ok(out)
        } else {
            Err(TemplateRenderingError::MissingKeys { missing_keys })
        }
    }

    /// Returns the names of the fields that are rendered in this template.
    pub fn get_fields(&self) -> Option<Vec<String>> {
        let parts: Vec<_> = self
            .parts
            .iter()
            .filter_map(|part| {
                if let Part::Reference(r) = part {
                    Some(r.to_owned())
                } else {
                    None
                }
            })
            .collect();
        (!parts.is_empty()).then_some(parts)
    }

    /// Returns a reference to the template string.
    pub fn get_ref(&self) -> &str {
        &self.src
    }

    /// Returns `true` if this template string has a length of zero, and `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.src.is_empty()
    }

    /// A dynamic template string contains sections that depend on the input event or time.
    pub const fn is_dynamic(&self) -> bool {
        !self.is_static
    }
}

/// One part of the template string after parsing.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Part {
    /// A literal piece of text to be copied verbatim into the output.
    Literal(String),
    /// A literal piece of text containing a time format string.
    Strftime(ParsedStrftime),
    /// A reference to the source event, to be copied from the relevant field or tag.
    Reference(String),
}

// Wrap the parsed time formatter in order to provide `impl Hash` and some convenience functions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ParsedStrftime(Box<[Item<'static>]>);

impl ParsedStrftime {
    fn parse(fmt: &str) -> Result<Self, TemplateParseError> {
        Ok(Self(
            StrftimeItems::new(fmt)
                .map(|item| match item {
                    // Box the references so they outlive the reference
                    Item::Space(space) => Item::OwnedSpace(space.into()),
                    Item::Literal(lit) => Item::OwnedLiteral(lit.into()),
                    // And copy all the others
                    Item::Fixed(f) => Item::Fixed(f),
                    Item::Numeric(num, pad) => Item::Numeric(num, pad),
                    Item::Error => Item::Error,
                    Item::OwnedSpace(space) => Item::OwnedSpace(space),
                    Item::OwnedLiteral(lit) => Item::OwnedLiteral(lit),
                })
                .map(|item| {
                    matches!(item, Item::Error)
                        .then(|| Err(TemplateParseError::StrftimeError))
                        .unwrap_or(Ok(item))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ))
    }

    fn is_dynamic(&self) -> bool {
        self.0.iter().any(|item| match item {
            Item::Fixed(_) => true,
            Item::Numeric(_, _) => true,
            Item::Error
            | Item::Space(_)
            | Item::OwnedSpace(_)
            | Item::Literal(_)
            | Item::OwnedLiteral(_) => false,
        })
    }

    fn as_items(&self) -> impl Iterator<Item = &Item<'static>> + Clone {
        self.0.iter()
    }

    fn reserve_size(&self) -> usize {
        self.0
            .iter()
            .map(|item| match item {
                Item::Literal(lit) => lit.len(),
                Item::OwnedLiteral(lit) => lit.len(),
                Item::Space(space) => space.len(),
                Item::OwnedSpace(space) => space.len(),
                Item::Error => 0,
                Item::Numeric(_, _) => 2,
                Item::Fixed(_) => 2,
            })
            .sum()
    }
}

fn parse_literal(src: &str) -> Result<Part, TemplateParseError> {
    let parsed = ParsedStrftime::parse(src)?;
    Ok(if parsed.is_dynamic() {
        Part::Strftime(parsed)
    } else {
        Part::Literal(src.to_string())
    })
}

// Pre-parse the template string into a series of parts to be filled in at render time.
fn parse_template(src: &str) -> Result<Vec<Part>, TemplateParseError> {
    let mut last_end = 0;
    let mut parts = Vec::new();
    for cap in RE.captures_iter(src) {
        let all = cap.get(0).expect("Capture 0 is always defined");
        if all.start() > last_end {
            parts.push(parse_literal(&src[last_end..all.start()])?);
        }

        let path = cap[1].trim().to_owned();

        // This checks the syntax, but doesn't yet store it for use later
        // see: https://github.com/vectordotdev/vector/issues/14864
        if parse_target_path(&path).is_err() {
            return Err(TemplateParseError::InvalidPathSyntax { path });
        }

        parts.push(Part::Reference(path));
        last_end = all.end();
    }
    if src.len() > last_end {
        parts.push(parse_literal(&src[last_end..])?);
    }

    Ok(parts)
}

fn render_metric_field<'a>(key: &str, metric: &'a Metric) -> Option<&'a str> {
    match key {
        "name" => Some(metric.name()),
        "namespace" => metric.namespace().map(Into::into),
        _ if key.starts_with("tags.") => metric.tags().and_then(|tags| tags.get(&key[5..])),
        _ => None,
    }
}

fn render_timestamp(
    items: &ParsedStrftime,
    event: EventRef<'_>,
    tz_offset: Option<FixedOffset>,
) -> String {
    let timestamp = match event {
        EventRef::Log(log) => log_schema()
            .timestamp_key_target_path()
            .and_then(|timestamp_key| {
                log.get(timestamp_key)
                    .and_then(Value::as_timestamp)
                    .copied()
            }),
        EventRef::Metric(metric) => metric.timestamp(),
        EventRef::Trace(trace) => {
            log_schema()
                .timestamp_key_target_path()
                .and_then(|timestamp_key| {
                    trace
                        .get(timestamp_key)
                        .and_then(Value::as_timestamp)
                        .copied()
                })
        }
    }
    .unwrap_or_else(Utc::now);

    match tz_offset {
        Some(offset) => timestamp
            .with_timezone(&offset)
            .format_with_items(items.as_items())
            .to_string(),
        None => timestamp
            .with_timezone(&chrono::Utc)
            .format_with_items(items.as_items())
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Offset, TimeZone, Utc};
    use chrono_tz::Tz;
    use vector_lib::lookup::{metadata_path, PathPrefix};
    use vector_lib::metric_tags;

    use super::*;
    use crate::event::{Event, LogEvent, MetricKind, MetricValue};

    #[test]
    fn get_fields() {
        let f1 = Template::try_from("{{ foo }}")
            .unwrap()
            .get_fields()
            .unwrap();
        let f2 = Template::try_from("{{ foo }}-{{ bar }}")
            .unwrap()
            .get_fields()
            .unwrap();
        let f3 = Template::try_from("nofield").unwrap().get_fields();
        let f4 = Template::try_from("%F").unwrap().get_fields();

        assert_eq!(f1, vec!["foo"]);
        assert_eq!(f2, vec!["foo", "bar"]);
        assert_eq!(f3, None);
        assert_eq!(f4, None);
    }

    #[test]
    fn is_dynamic() {
        assert!(Template::try_from("/kube-demo/%F").unwrap().is_dynamic());
        assert!(!Template::try_from("/kube-demo/echo").unwrap().is_dynamic());
        assert!(Template::try_from("/kube-demo/{{ foo }}")
            .unwrap()
            .is_dynamic());
        assert!(Template::try_from("/kube-demo/{{ foo }}/%F")
            .unwrap()
            .is_dynamic());
    }

    #[test]
    fn render_log_static() {
        let event = Event::Log(LogEvent::from("hello world"));
        let template = Template::try_from("foo").unwrap();

        assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
    }

    #[test]
    fn render_log_metadata() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(metadata_path!("metadata_key"), "metadata_value");
        let template = Template::try_from("{{%metadata_key}}").unwrap();

        assert_eq!(Ok(Bytes::from("metadata_value")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_with_prefix() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("abcd-{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_with_postfix() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("{{log_stream}}-abcd").unwrap();

        assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_missing_key() {
        let event = Event::Log(LogEvent::from("hello world"));
        let template = Template::try_from("{{log_stream}}-{{foo}}").unwrap();

        assert_eq!(
            Err(TemplateRenderingError::MissingKeys {
                missing_keys: vec!["log_stream".to_string(), "foo".to_string()]
            }),
            template.render(&event)
        );
    }

    #[test]
    fn render_log_dynamic_multiple_keys() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("foo", "bar");
        event.as_mut_log().insert("baz", "quux");
        let template = Template::try_from("stream-{{foo}}-{{baz}}.log").unwrap();

        assert_eq!(
            Ok(Bytes::from("stream-bar-quux.log")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_weird_junk() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("foo", "bar");
        event.as_mut_log().insert("baz", "quux");
        let template = Template::try_from(r"{stream}{\{{}}}-{{foo}}-{{baz}}.log").unwrap();

        assert_eq!(
            Ok(Bytes::from(r"{stream}{\{{}}}-bar-quux.log")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_timestamp_strftime_style() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

        let template = Template::try_from("abcd-%F").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
    }

    #[test]
    fn render_log_timestamp_multiple_strftime_style() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

        let template = Template::try_from("abcd-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("abcd-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_strftime() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("foo", "butts");
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let template = Template::try_from("{{ foo }}-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("butts-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_nested_strftime() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("format", "%F");
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let template = Template::try_from("nested {{ format }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested %F 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_reverse_nested_strftime() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert("\"%F\"", "foo");
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let template = Template::try_from("nested {{ \"%F\" }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested foo 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_metric_timestamp() {
        let template = Template::try_from("timestamp %F %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("timestamp 2002-03-04 05:06:07")),
            template.render(&sample_metric())
        );
    }

    #[test]
    fn render_metric_with_tags() {
        let template = Template::try_from("name={{name}} component={{tags.component}}").unwrap();
        let metric = sample_metric().with_tags(Some(metric_tags!(
            "test" => "true",
            "component" => "template",
        )));
        assert_eq!(
            Ok(Bytes::from("name=a-counter component=template")),
            template.render(&metric)
        );
    }

    #[test]
    fn render_metric_without_tags() {
        let template = Template::try_from("name={{name}} component={{tags.component}}").unwrap();
        assert_eq!(
            Err(TemplateRenderingError::MissingKeys {
                missing_keys: vec!["tags.component".into()]
            }),
            template.render(&sample_metric())
        );
    }

    #[test]
    fn render_metric_with_namespace() {
        let template = Template::try_from("namespace={{namespace}} name={{name}}").unwrap();
        let metric = sample_metric().with_namespace(Some("vector-test"));
        assert_eq!(
            Ok(Bytes::from("namespace=vector-test name=a-counter")),
            template.render(&metric)
        );
    }

    #[test]
    fn render_metric_without_namespace() {
        let template = Template::try_from("namespace={{namespace}} name={{name}}").unwrap();
        let metric = sample_metric();
        assert_eq!(
            Err(TemplateRenderingError::MissingKeys {
                missing_keys: vec!["namespace".into()]
            }),
            template.render(&metric)
        );
    }

    #[test]
    fn render_log_with_timezone() {
        let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

        let template = Template::try_from("vector-%Y-%m-%d-%H.log").unwrap();
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let tz: Tz = "Asia/Singapore".parse().unwrap();
        let offset = Some(Utc::now().with_timezone(&tz).offset().fix());
        assert_eq!(
            Ok(Bytes::from("vector-2001-02-03-12.log")),
            template.with_tz_offset(offset).render(&event)
        );
    }

    fn sample_metric() -> Metric {
        Metric::new(
            "a-counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.1 },
        )
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2002, 3, 4, 5, 6, 7)
                .single()
                .expect("invalid timestamp"),
        ))
    }

    #[test]
    fn strftime_error() {
        assert_eq!(
            Template::try_from("%E").unwrap_err(),
            TemplateParseError::StrftimeError
        );
    }
}
