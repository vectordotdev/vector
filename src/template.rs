use crate::{
    config::log_schema,
    event::{Metric, Value},
    Event,
};
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
use std::borrow::Cow;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

lazy_static! {
    static ref RE: Regex = Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap();
}

#[derive(Debug, Default, Clone)]
pub struct Template {
    src: String,
    has_ts: bool,
    has_fields: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateError {
    StrftimeError,
}

impl Error for TemplateError {}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StrftimeError => write!(f, "Invalid strftime item"),
        }
    }
}

impl TryFrom<&str> for Template {
    type Error = TemplateError;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        Template::try_from(Cow::Borrowed(src))
    }
}

impl TryFrom<String> for Template {
    type Error = TemplateError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        Template::try_from(Cow::Owned(src))
    }
}

impl TryFrom<PathBuf> for Template {
    type Error = TemplateError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        Template::try_from(p.to_string_lossy().into_owned())
    }
}

impl TryFrom<Cow<'_, str>> for Template {
    type Error = TemplateError;

    fn try_from(src: Cow<'_, str>) -> Result<Self, Self::Error> {
        let (has_error, is_dynamic) = StrftimeItems::new(&src)
            .fold((false, false), |pair, item| {
                (pair.0 || is_error(&item), pair.1 || is_dynamic(&item))
            });
        if has_error {
            Err(TemplateError::StrftimeError)
        } else {
            Ok(Template {
                has_fields: RE.is_match(&src),
                src: src.into_owned(),
                has_ts: is_dynamic,
            })
        }
    }
}

fn is_error(item: &Item) -> bool {
    matches!(item, Item::Error)
}

fn is_dynamic(item: &Item) -> bool {
    match item {
        Item::Error => false,
        Item::Fixed(_) => true,
        Item::Numeric(_, _) => true,
        Item::Space(_) | Item::OwnedSpace(_) => false,
        Item::Literal(_) | Item::OwnedLiteral(_) => false,
    }
}

impl Template {
    pub fn render(&self, event: &Event) -> Result<Bytes, Vec<String>> {
        self.render_string(event).map(Into::into)
    }

    pub fn render_string(&self, event: &Event) -> Result<String, Vec<String>> {
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

    pub fn is_dynamic(&self) -> bool {
        self.has_fields || self.has_ts
    }

    pub fn get_ref(&self) -> &str {
        &self.src
    }
}

fn render_fields(src: &str, event: &Event) -> Result<String, Vec<String>> {
    let mut missing_fields = Vec::new();
    let out = RE
        .replace_all(src, |caps: &Captures<'_>| {
            let key = caps
                .get(1)
                .map(|s| s.as_str().trim())
                .expect("src should match regex");
            match event {
                Event::Log(log) => log.get(&key).map(|val| val.to_string_lossy()),
                Event::Metric(metric) => render_metric_field(key, metric),
            }
            .unwrap_or_else(|| {
                missing_fields.push(key.to_owned());
                String::new()
            })
        })
        .into_owned();
    if missing_fields.is_empty() {
        Ok(out)
    } else {
        Err(missing_fields)
    }
}

fn render_metric_field(key: &str, metric: &Metric) -> Option<String> {
    match key {
        "name" => Some(metric.name().into()),
        "namespace" => metric.namespace().map(Into::into),
        _ if key.starts_with("tags.") => metric
            .series
            .tags
            .as_ref()
            .and_then(|tags| tags.get(&key[5..]).cloned()),
        _ => None,
    }
}

fn render_timestamp(src: &str, event: &Event) -> String {
    let timestamp = match event {
        Event::Log(log) => log
            .get(log_schema().timestamp_key())
            .and_then(Value::as_timestamp),
        Event::Metric(metric) => metric.data.timestamp.as_ref(),
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
    use super::*;
    use crate::event::{MetricKind, MetricValue};
    use chrono::TimeZone;
    use shared::btreemap;

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
        assert_eq!(
            true,
            Template::try_from("/kube-demo/%F").unwrap().is_dynamic()
        );
        assert_eq!(
            false,
            Template::try_from("/kube-demo/echo").unwrap().is_dynamic()
        );
        assert_eq!(
            true,
            Template::try_from("/kube-demo/{{ foo }}")
                .unwrap()
                .is_dynamic()
        );
        assert_eq!(
            true,
            Template::try_from("/kube-demo/{{ foo }}/%F")
                .unwrap()
                .is_dynamic()
        );
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
            Err(vec!["log_stream".to_string(), "foo".to_string()]),
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
            template.render(&sample_metric().into())
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
            template.render(&metric.into())
        );
    }

    #[test]
    fn render_metric_without_tags() {
        let template = Template::try_from("name={{name}} component={{tags.component}}").unwrap();
        assert_eq!(
            Err(vec!["tags.component".into()]),
            template.render(&sample_metric().into())
        );
    }

    #[test]
    fn render_metric_with_namespace() {
        let template = Template::try_from("namespace={{namespace}} name={{name}}").unwrap();
        let metric = sample_metric().with_namespace(Some("vector-test"));
        assert_eq!(
            Ok(Bytes::from("namespace=vector-test name=a-counter")),
            template.render(&metric.into())
        );
    }

    #[test]
    fn render_metric_without_namespace() {
        let template = Template::try_from("namespace={{namespace}} name={{name}}").unwrap();
        let metric = sample_metric();
        assert_eq!(
            Err(vec!["namespace".into()]),
            template.render(&metric.into())
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
            TemplateError::StrftimeError
        );
    }
}
