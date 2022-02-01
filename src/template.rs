use std::{borrow::Cow, convert::TryFrom, fmt, hash::Hash, path::PathBuf};

use bytes::Bytes;
use chrono::{
    format::{strftime::StrftimeItems, Item},
    Utc,
};
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use serde::{
    de::{self, Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};
use snafu::Snafu;

use crate::{
    config::log_schema,
    event::{EventRef, Metric, Value},
};

lazy_static! {
    static ref RE: Regex = Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap();
}

#[derive(Debug, Default, Eq, PartialEq, Hash, Clone)]
pub struct Template {
    src: String,
    has_ts: bool,
    has_fields: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Snafu)]
pub enum TemplateParseError {
    #[snafu(display("Invalid strftime item"))]
    StrftimeError,
}

#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum TemplateRenderingError {
    #[snafu(display("Missing fields on event: {:?}", missing_keys))]
    MissingKeys { missing_keys: Vec<String> },
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
        let (has_error, is_dynamic) = StrftimeItems::new(&src)
            .fold((false, false), |(error, dynamic), item| {
                (error || is_error(&item), dynamic || is_dynamic(&item))
            });
        if has_error {
            Err(TemplateParseError::StrftimeError)
        } else {
            Ok(Template {
                has_fields: RE.is_match(&src),
                src: src.into_owned(),
                has_ts: is_dynamic,
            })
        }
    }
}

const fn is_error(item: &Item) -> bool {
    matches!(item, Item::Error)
}

const fn is_dynamic(item: &Item) -> bool {
    match item {
        Item::Fixed(_) => true,
        Item::Numeric(_, _) => true,
        Item::Error => false,
        Item::Space(_) | Item::OwnedSpace(_) => false,
        Item::Literal(_) | Item::OwnedLiteral(_) => false,
    }
}

impl Template {
    pub fn render<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<Bytes, TemplateRenderingError> {
        self.render_string(event.into()).map(Into::into)
    }

    pub fn render_string<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<String, TemplateRenderingError> {
        let event = event.into();
        match (self.has_fields, self.has_ts) {
            (false, false) => Ok(self.src.clone()),
            (true, false) => render_fields(&self.src, event),
            (false, true) => Ok(render_timestamp(&self.src, event)),
            (true, true) => {
                let tmp = render_fields(&self.src, event)?;
                Ok(render_timestamp(&tmp, event))
            }
        }
    }

    pub fn get_fields(&self) -> Option<Vec<String>> {
        if self.has_fields {
            RE.captures_iter(&self.src)
                .map(|c| {
                    c.get(1)
                        .map(|s| s.as_str().trim().to_string())
                        .expect("src should match regex")
                })
                .collect::<Vec<_>>()
                .into()
        } else {
            None
        }
    }

    pub const fn is_dynamic(&self) -> bool {
        self.has_fields || self.has_ts
    }

    pub fn get_ref(&self) -> &str {
        &self.src
    }
}

fn render_fields<'a>(src: &str, event: EventRef<'a>) -> Result<String, TemplateRenderingError> {
    let mut missing_keys = Vec::new();
    let out = RE
        .replace_all(src, |caps: &Captures<'_>| {
            let key = caps
                .get(1)
                .map(|s| s.as_str().trim())
                .expect("src should match regex");
            match event {
                EventRef::Log(log) => log.get(&key).map(|val| val.to_string_lossy()),
                EventRef::Metric(metric) => render_metric_field(key, metric),
            }
            .unwrap_or_else(|| {
                missing_keys.push(key.to_owned());
                String::new()
            })
        })
        .into_owned();
    if missing_keys.is_empty() {
        Ok(out)
    } else {
        Err(TemplateRenderingError::MissingKeys { missing_keys })
    }
}

fn render_metric_field(key: &str, metric: &Metric) -> Option<String> {
    match key {
        "name" => Some(metric.name().into()),
        "namespace" => metric.namespace().map(Into::into),
        _ if key.starts_with("tags.") => {
            metric.tags().and_then(|tags| tags.get(&key[5..]).cloned())
        }
        _ => None,
    }
}

fn render_timestamp(src: &str, event: EventRef<'_>) -> String {
    let timestamp = match event {
        EventRef::Log(log) => log
            .get(log_schema().timestamp_key())
            .and_then(Value::as_timestamp)
            .copied(),
        EventRef::Metric(metric) => metric.timestamp(),
    };
    if let Some(ts) = timestamp {
        ts.format(src).to_string()
    } else {
        Utc::now().format(src).to_string()
    }
}

impl<'de> Deserialize<'de> for Template {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(TemplateVisitor)
    }
}

struct TemplateVisitor;

impl<'de> Visitor<'de> for TemplateVisitor {
    type Value = Template;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a string")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Template::try_from(s).map_err(de::Error::custom)
    }
}

impl Serialize for Template {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // TODO: determine if we should serialize this as a struct or just the
        // str.
        serializer.serialize_str(&self.src)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use vector_common::btreemap;

    use super::*;
    use crate::event::{Event, MetricKind, MetricValue};

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
        let event = Event::from("hello world");
        let template = Template::try_from("foo").unwrap();

        assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_with_prefix() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("abcd-{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_with_postfix() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("{{log_stream}}-abcd").unwrap();

        assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_missing_key() {
        let event = Event::from("hello world");
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
        let mut event = Event::from("hello world");
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
        let mut event = Event::from("hello world");
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
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert(log_schema().timestamp_key(), ts);

        let template = Template::try_from("abcd-%F").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
    }

    #[test]
    fn render_log_timestamp_multiple_strftime_style() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert(log_schema().timestamp_key(), ts);

        let template = Template::try_from("abcd-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("abcd-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("foo", "butts");
        event.as_mut_log().insert(log_schema().timestamp_key(), ts);

        let template = Template::try_from("{{ foo }}-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("butts-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_nested_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("format", "%F");
        event.as_mut_log().insert(log_schema().timestamp_key(), ts);

        let template = Template::try_from("nested {{ format }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested 2001-02-03 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_reverse_nested_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("%F", "foo");
        event.as_mut_log().insert(log_schema().timestamp_key(), ts);

        let template = Template::try_from("nested {{ %F }} %T").unwrap();

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
        let metric = sample_metric().with_tags(Some(
            btreemap! { "test" => "true", "component" => "template" },
        ));
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

    fn sample_metric() -> Metric {
        Metric::new(
            "a-counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.1 },
        )
        .with_timestamp(Some(Utc.ymd(2002, 3, 4).and_hms(5, 6, 7)))
    }

    #[test]
    fn strftime_error() {
        assert_eq!(
            Template::try_from("%E").unwrap_err(),
            TemplateParseError::StrftimeError
        );
    }
}
