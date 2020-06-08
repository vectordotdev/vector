use crate::{
    event::{self, Value},
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
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use string_cache::DefaultAtom as Atom;

lazy_static! {
    static ref RE: Regex = Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap();
}

#[derive(Debug, Default, Clone)]
pub struct Template {
    src: String,
    src_bytes: Bytes,
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
        let (has_error, is_dynamic) = StrftimeItems::new(src).fold((false, false), |pair, item| {
            (pair.0 || is_error(&item), pair.1 || is_dynamic(&item))
        });
        match has_error {
            true => Err(TemplateError::StrftimeError),
            false => Ok(Template {
                src: src.into(),
                src_bytes: src.into(),
                has_ts: is_dynamic,
                has_fields: RE.is_match(src),
            }),
        }
    }
}

impl TryFrom<PathBuf> for Template {
    type Error = TemplateError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        Template::try_from(&*p.to_string_lossy())
    }
}

impl TryFrom<String> for Template {
    type Error = TemplateError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Template::try_from(s.as_str())
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
    pub fn render(&self, event: &Event) -> Result<Bytes, Vec<Atom>> {
        match (self.has_fields, self.has_ts) {
            (false, false) => Ok(self.src_bytes.clone()),
            (true, false) => render_fields(&self.src, event).map(Bytes::from),
            (false, true) => Ok(render_timestamp(&self.src, event).into()),
            (true, true) => {
                let tmp = render_fields(&self.src, event)?;
                Ok(render_timestamp(&tmp, event).into())
            }
        }
    }

    pub fn render_string(&self, event: &Event) -> Result<String, Vec<Atom>> {
        self.render(event)
            .map(|bytes| String::from_utf8(Vec::from(bytes.as_ref())).expect("this is a bug"))
    }

    pub fn get_fields(&self) -> Option<Vec<Atom>> {
        if self.has_fields {
            RE.captures_iter(&self.src)
                .map(|c| {
                    c.get(1)
                        .map(|s| Atom::from(s.as_str().trim()))
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

    pub fn get_ref(&self) -> &Bytes {
        &self.src_bytes
    }
}

fn render_fields(src: &str, event: &Event) -> Result<String, Vec<Atom>> {
    let mut missing_fields = Vec::new();
    let out = RE
        .replace_all(src, |caps: &Captures<'_>| {
            let key = caps
                .get(1)
                .map(|s| Atom::from(s.as_str().trim()))
                .expect("src should match regex");
            if let Some(val) = event.as_log().get(&key) {
                val.to_string_lossy()
            } else {
                missing_fields.push(key.clone());
                String::new()
            }
        })
        .into_owned();
    if missing_fields.is_empty() {
        Ok(out)
    } else {
        Err(missing_fields)
    }
}

fn render_timestamp(src: &str, event: &Event) -> String {
    let timestamp = match event {
        Event::Log(log) => log
            .get(&event::log_schema().timestamp_key())
            .and_then(Value::as_timestamp),
        _ => None,
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
    use chrono::TimeZone;

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

        assert_eq!(f1, vec![Atom::from("foo")]);
        assert_eq!(f2, vec![Atom::from("foo"), Atom::from("bar")]);
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
    fn render_static() {
        let event = Event::from("hello world");
        let template = Template::try_from("foo").unwrap();

        assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
    }

    #[test]
    fn render_dynamic() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
    }

    #[test]
    fn render_dynamic_with_prefix() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("abcd-{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
    }

    #[test]
    fn render_dynamic_with_postfix() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("log_stream", "stream");
        let template = Template::try_from("{{log_stream}}-abcd").unwrap();

        assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
    }

    #[test]
    fn render_dynamic_missing_key() {
        let event = Event::from("hello world");
        let template = Template::try_from("{{log_stream}}-{{foo}}").unwrap();

        assert_eq!(
            Err(vec![Atom::from("log_stream"), Atom::from("foo")]),
            template.render(&event)
        );
    }

    #[test]
    fn render_dynamic_multiple_keys() {
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
    fn render_dynamic_weird_junk() {
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
    fn render_timestamp_strftime_style() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert(crate::event::log_schema().timestamp_key().clone(), ts);

        let template = Template::try_from("abcd-%F").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
    }

    #[test]
    fn render_timestamp_multiple_strftime_style() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert(crate::event::log_schema().timestamp_key().clone(), ts);

        let template = Template::try_from("abcd-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("abcd-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_with_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("foo", "butts");
        event
            .as_mut_log()
            .insert(crate::event::log_schema().timestamp_key().clone(), ts);

        let template = Template::try_from("{{ foo }}-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("butts-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_with_nested_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("format", "%F");
        event
            .as_mut_log()
            .insert(crate::event::log_schema().timestamp_key().clone(), ts);

        let template = Template::try_from("nested {{ format }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested 2001-02-03 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_with_reverse_nested_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("%F", "foo");
        event
            .as_mut_log()
            .insert(crate::event::log_schema().timestamp_key().clone(), ts);

        let template = Template::try_from("nested {{ %F }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested foo 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn strftime_error() {
        assert_eq!(
            Template::try_from("%E").unwrap_err(),
            TemplateError::StrftimeError
        );
    }
}
