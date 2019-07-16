use crate::Event;
use bytes::Bytes;
use regex::bytes::{Captures, Regex};
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Clone)]
pub enum Template {
    Static(Bytes),
    Dynamic(Regex, Bytes, Atom),
}

impl From<&str> for Template {
    fn from(src: &str) -> Template {
        let r = Regex::new(r"\{\{(?P<key>\D+)\}\}").unwrap();

        if let Some(cap) = r.captures(src.as_bytes()) {
            if let Some(m) = cap.name("key") {
                // TODO(lucio): clean up unwrap
                let key = String::from_utf8(Vec::from(m.as_bytes())).unwrap();
                return Template::Dynamic(r, src.into(), key.into());
            }
        }

        Template::Static(src.into())
    }
}

impl Template {
    // TODO: return error
    pub fn render(&self, event: &Event) -> Result<Bytes, &Atom> {
        match self {
            Template::Static(g) => Ok(g.clone()),
            Template::Dynamic(regex, src, key) => {
                if let Some(val) = event.as_log().get(&key) {
                    let cap = regex.replace(src, |_cap: &Captures| val.as_bytes().clone());
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
    fn parse_dynamic() {
        if let Template::Dynamic(_, _, key) = Template::from("{{some_key}}") {
            assert_eq!(key, "some_key".to_string());
        } else {
            panic!("Expected Template::Dynamic");
        }
    }

    #[test]
    fn parse_static() {
        if let Template::Static(key) = Template::from("static_key") {
            assert_eq!(key, "static_key".to_string());
        } else {
            panic!("Expected Template::Static");
        }
    }

    #[test]
    fn render_static() {
        let event = Event::from("hello world");
        let template = Template::Static("foo".into());

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
        let template = Template::from("{{log_stream}}");

        assert_eq!(Err(&Atom::from("log_stream")), template.render(&event));
    }
}
