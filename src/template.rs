use crate::{
    event::{self, ValueKind},
    Event,
};
use bytes::Bytes;
use chrono::{format::strftime::StrftimeItems, Utc};
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use string_cache::DefaultAtom as Atom;

lazy_static! {
    static ref RE: Regex = Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap();
}

#[derive(Debug, Clone)]
pub struct Template {
    src: String,
    src_bytes: Bytes,
    has_ts: bool,
    has_fields: bool,
}

impl From<&str> for Template {
    fn from(src: &str) -> Template {
        Template {
            src: src.into(),
            src_bytes: src.into(),
            has_ts: StrftimeItems::new(src).count() > 0,
            has_fields: RE.is_match(src),
        }
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
}

fn render_fields(src: &str, event: &Event) -> Result<String, Vec<Atom>> {
    let mut missing_fields = Vec::new();
    let out = RE
        .replace_all(src, |caps: &Captures| {
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
        Event::Log(log) => log.get(&event::TIMESTAMP).and_then(ValueKind::as_timestamp),
        _ => None,
    };
    if let Some(ts) = timestamp {
        ts.format(src).to_string()
    } else {
        Utc::now().format(src).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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

    #[test]
    fn render_timestamp_strftime_style() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit(crate::event::TIMESTAMP.clone(), ts.into());

        let template = Template::from("abcd-%F");

        assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
    }

    #[test]
    fn render_timestamp_multiple_strftime_style() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit(crate::event::TIMESTAMP.clone(), ts.into());

        let template = Template::from("abcd-%F_%T");

        assert_eq!(
            Ok(Bytes::from("abcd-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_with_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("foo".into(), "butts".into());
        event
            .as_mut_log()
            .insert_implicit(crate::event::TIMESTAMP.clone(), ts.into());

        let template = Template::from("{{ foo }}-%F_%T");

        assert_eq!(
            Ok(Bytes::from("butts-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_with_nested_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("format".into(), "%F".into());
        event
            .as_mut_log()
            .insert_implicit(crate::event::TIMESTAMP.clone(), ts.into());

        let template = Template::from("nested {{ format }} %T");

        assert_eq!(
            Ok(Bytes::from("nested 2001-02-03 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_dynamic_with_reverse_nested_strftime() {
        let ts = Utc.ymd(2001, 2, 3).and_hms(4, 5, 6);

        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("%F".into(), "foo".into());
        event
            .as_mut_log()
            .insert_implicit(crate::event::TIMESTAMP.clone(), ts.into());

        let template = Template::from("nested {{ %F }} %T");

        assert_eq!(
            Ok(Bytes::from("nested foo 04:05:06")),
            template.render(&event)
        )
    }
}
