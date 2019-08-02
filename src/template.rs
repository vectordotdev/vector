use crate::Event;
use bytes::Bytes;
use lazy_static::lazy_static;
use regex::bytes::{Captures, Regex};
use serde::{
    de::{self, Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};
use std::fmt;
use string_cache::DefaultAtom as Atom;

lazy_static! {
    static ref RE: Regex = Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap();
}

#[derive(Debug, Clone)]
pub struct Template {
    src: Bytes,
    dynamic: bool,
}

impl From<&str> for Template {
    fn from(src: &str) -> Template {
        Template {
            src: src.into(),
            dynamic: RE.is_match(src.as_bytes()),
        }
    }
}

impl Template {
    pub fn render(&self, event: &Event) -> Result<Bytes, Vec<Atom>> {
        if !self.dynamic {
            return Ok(self.src.clone());
        }

        let mut missing_fields = Vec::new();
        let out = RE
            .replace_all(self.src.as_ref(), |caps: &Captures| {
                let key = caps
                    .get(1)
                    .and_then(|m| std::str::from_utf8(m.as_bytes()).ok())
                    .map(|s| Atom::from(s))
                    .expect("src should match regex and keys should be utf8");
                if let Some(val) = event.as_log().get(&key) {
                    val.to_string_lossy()
                } else {
                    missing_fields.push(key.clone());
                    String::new()
                }
            })
            .into_owned();
        if missing_fields.is_empty() {
            Ok(out.into())
        } else {
            Err(missing_fields)
        }
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

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a string")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Template::from(s))
    }
}

impl Serialize for Template {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // TODO: determine if we should serialize this as a struct or just the
        // bytes.
        serializer.serialize_bytes(&self.src[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_static() {
        let event = Event::from("hello world");
        let template = Template::from("foo");

        assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
    }

    #[test]
    fn render_dynamic() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());
        let template = Template::from("{{log_stream}}");

        assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
    }

    #[test]
    fn render_dynamic_with_prefix() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());
        let template = Template::from("abcd-{{log_stream}}");

        assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
    }

    #[test]
    fn render_dynamic_with_postfix() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());
        let template = Template::from("{{log_stream}}-abcd");

        assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
    }

    #[test]
    fn render_dynamic_missing_key() {
        let event = Event::from("hello world");
        let template = Template::from("{{log_stream}}-{{foo}}");

        assert_eq!(
            Err(vec![Atom::from("log_stream"), Atom::from("foo")]),
            template.render(&event)
        );
    }

    #[test]
    fn render_dynamic_multiple_keys() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("foo".into(), "bar".into());
        event
            .as_mut_log()
            .insert_implicit("baz".into(), "quux".into());
        let template = Template::from("stream-{{foo}}-{{baz}}.log");

        assert_eq!(
            Ok(Bytes::from("stream-bar-quux.log")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_weird_junk() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("foo".into(), "bar".into());
        event
            .as_mut_log()
            .insert_implicit("baz".into(), "quux".into());
        let template = Template::from(r"{stream}{\{{}}}-{{foo}}-{{baz}}.log");

        assert_eq!(
            Ok(Bytes::from(r"{stream}{\{{}}}-bar-quux.log")),
            template.render(&event)
        )
    }
}
