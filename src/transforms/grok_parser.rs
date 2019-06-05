use super::Transform;
use crate::event::{self, Event};
use grok::Pattern;
use serde::{Deserialize, Serialize};
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct GrokParserConfig {
    pub pattern: String,
    pub field: Option<Atom>,
    #[derivative(Default(value = "true"))]
    pub drop_field: bool,
}

#[typetag::serde(name = "grok_parser")]
impl crate::topology::config::TransformConfig for GrokParserConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        let field = if let Some(field) = &self.field {
            field
        } else {
            &event::MESSAGE
        };

        let mut grok = grok::Grok::with_patterns();

        grok.compile(&self.pattern, true)
            .map_err(|err| err.to_string())
            .map::<Box<dyn Transform>, _>(|p| {
                Box::new(GrokParser {
                    pattern: p,
                    field: field.clone(),
                    drop_field: self.drop_field,
                })
            })
    }
}

pub struct GrokParser {
    pattern: Pattern,
    field: Atom,
    drop_field: bool,
}

impl Transform for GrokParser {
    fn transform(&self, event: Event) -> Option<Event> {
        let mut event = event.into_log();
        let value = event.get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = value {
            if let Some(matches) = self.pattern.match_against(&value) {
                for (name, value) in matches.iter() {
                    event.insert_explicit(name.into(), value.into());
                }

                if self.drop_field {
                    event.remove(&self.field);
                }
            } else {
                debug!(message = "No fields captured from grok pattern.");
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
            );
        }

        Some(Event::Log(event))
    }
}

#[cfg(test)]
mod tests {
    use super::GrokParserConfig;
    use crate::{event, topology::config::TransformConfig, Event};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn grok_parser_adds_parsed_fields_to_event() {
        let event = Event::from(r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#);
        let parser = GrokParserConfig {
            pattern: String::from("%{HTTPD_COMMONLOG}"),
            ..Default::default()
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        let expected = json!({
            "clientip": "109.184.11.34",
            "ident": "-",
            "auth": "-",
            "timestamp": "12/Dec/2015:18:32:56 +0100",
            "verb": "GET",
            "request": "/administrator/",
            "httpversion": "1.1",
            "rawrequest": "",
            "response": "200",
            "bytes": "4263",
        });

        assert_eq!(
            expected,
            serde_json::to_value(&event.as_log().all_fields()).unwrap()
        );
    }

    #[test]
    fn grok_parser_does_nothing_on_no_match() {
        let event = Event::from(r#"help i'm stuck in an http server"#);
        let parser = GrokParserConfig {
            pattern: String::from("%{HTTPD_COMMONLOG}"),
            ..Default::default()
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap().into_log();

        assert_eq!(2, event.keys().count());
        assert_eq!(
            event::ValueKind::from("help i'm stuck in an http server"),
            event[&event::MESSAGE]
        );
        assert!(event[&event::TIMESTAMP].to_string_lossy().len() > 0);
    }

    #[test]
    fn grok_parser_can_not_drop_parsed_field() {
        let event = Event::from(r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#);
        let parser = GrokParserConfig {
            pattern: String::from("%{HTTPD_COMMONLOG}"),
            drop_field: false,
            ..Default::default()
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        let expected = json!({
            "clientip": "109.184.11.34",
            "ident": "-",
            "auth": "-",
            "timestamp": "12/Dec/2015:18:32:56 +0100",
            "verb": "GET",
            "request": "/administrator/",
            "httpversion": "1.1",
            "rawrequest": "",
            "response": "200",
            "bytes": "4263",
            "message": r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
        });

        assert_eq!(
            expected,
            serde_json::to_value(&event.as_log().all_fields()).unwrap()
        );
    }

    #[test]
    fn grok_parser_does_nothing_on_missing_field() {
        let event = Event::from("i am the only field");
        let parser = GrokParserConfig {
            pattern: String::from("^(?<foo>.*)"),
            field: Some("bar".into()),
            ..Default::default()
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap().into_log();

        assert_eq!(2, event.keys().count());
        assert_eq!(
            event::ValueKind::from("i am the only field"),
            event[&event::MESSAGE]
        );
        assert!(event[&event::TIMESTAMP].to_string_lossy().len() > 0);
    }
}
