use crate::Event;
use bytes::Bytes;
use regex::bytes::{Captures, Regex};
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Clone)]
pub enum Template {
    /// A static field that doesn't create dynamic partitions
    Static(Bytes),
    /// Represents the ability to extract a key/value from the event
    /// via the provided interpolated stream name.
    Field(Regex, Bytes, Atom),
}

pub fn parse_template(src: &str) -> Template {
    let r = Regex::new(r"\{\{(?P<key>\D+)\}\}").unwrap();

    if let Some(cap) = r.captures(src.as_bytes()) {
        if let Some(m) = cap.name("key") {
            // TODO(lucio): clean up unwrap
            let key = String::from_utf8(Vec::from(m.as_bytes())).unwrap();
            return Template::Field(r, src.into(), key.into());
        }
    }

    Template::Static(src.into())
}

impl Template {
    // TODO: return error
    pub fn render(&self, event: &Event) -> Result<Bytes, &Atom> {
        match self {
            Template::Static(g) => Ok(g.clone()),
            Template::Field(regex, stream, key) => {
                if let Some(val) = event.as_log().get(&key) {
                    let cap = regex.replace(stream, |_cap: &Captures| val.as_bytes().clone());
                    Ok(Bytes::from(&cap[..]))
                } else {
                    Err(key)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_event() {
        if let Template::Field(_, _, key) = parse_template("{{some_key}}") {
            assert_eq!(key, "some_key".to_string());
        } else {
            panic!("Expected Template::Field");
        }
    }

    #[test]
    fn interpolate_static() {
        if let Template::Static(key) = parse_template("static_key") {
            assert_eq!(key, "static_key".to_string());
        } else {
            panic!("Expected Template::Static");
        }
    }

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let template = Template::Static("foo".into());

        assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
    }

    #[test]
    fn partition_event() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());
        let template = parse_template("{{log_stream}}");

        assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
    }

    #[test]
    fn partition_event_with_prefix() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());
        let template = parse_template("abcd-{{log_stream}}");

        assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
    }

    #[test]
    fn partition_event_with_postfix() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());
        let template = parse_template("{{log_stream}}-abcd");

        assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
    }

    #[test]
    fn partition_no_key_event() {
        let event = Event::from("hello world");
        let template = parse_template("{{log_stream}}");

        assert_eq!(Err(&Atom::from("log_stream")), template.render(&event));
    }
}
