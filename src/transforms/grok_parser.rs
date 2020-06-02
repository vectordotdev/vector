use super::Transform;
use crate::{
    event::{self, Event, PathComponent, PathIter},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    types::{parse_conversion_map_no_atoms, Conversion},
};
use grok::Pattern;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid grok pattern: {}", source))]
    InvalidGrok { source: grok::Error },
}

#[derive(Deserialize, Serialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct GrokParserConfig {
    pub pattern: String,
    pub field: Option<Atom>,
    #[derivative(Default(value = "true"))]
    pub drop_field: bool,
    pub types: HashMap<String, String>,
}

inventory::submit! {
    TransformDescription::new::<GrokParserConfig>("grok_parser")
}

#[typetag::serde(name = "grok_parser")]
impl TransformConfig for GrokParserConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());

        let mut grok = grok::Grok::with_patterns();

        let types = parse_conversion_map_no_atoms(&self.types)?;

        Ok(grok
            .compile(&self.pattern, true)
            .map::<Box<dyn Transform>, _>(|p| {
                Box::new(GrokParser {
                    pattern: p,
                    field: field.clone(),
                    drop_field: self.drop_field,
                    types,
                    paths: HashMap::new(),
                })
            })
            .context(InvalidGrok)?)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "grok_parser"
    }
}

pub struct GrokParser {
    pattern: Pattern,
    field: Atom,
    drop_field: bool,
    types: HashMap<String, Conversion>,
    paths: HashMap<String, Vec<PathComponent>>,
}

impl Transform for GrokParser {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut event = event.into_log();
        let value = event.get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = value {
            if let Some(matches) = self.pattern.match_against(&value) {
                let drop_field = self.drop_field && !matches.get(&self.field).is_some();
                for (name, value) in matches.iter() {
                    let conv = self.types.get(name).unwrap_or(&Conversion::Bytes);
                    match conv.convert(value.into()) {
                        Ok(value) => {
                            if let Some(path) = self.paths.get(name) {
                                event.insert_path(path.to_vec(), value);
                            } else {
                                let path = PathIter::new(name).collect::<Vec<_>>();
                                self.paths.insert(name.to_string(), path.clone());
                                event.insert_path(path, value);
                            }
                        }
                        Err(error) => {
                            debug!(
                                message = "Could not convert types.",
                                %name,
                                %error,
                                rate_limit_secs = 30,
                            );
                        }
                    }
                }

                if drop_field {
                    event.remove(&self.field);
                }
            } else {
                debug!(message = "No fields captured from grok pattern.");
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
                rate_limit_secs = 30,
            );
        }

        Some(Event::Log(event))
    }
}

#[cfg(test)]
mod tests {
    use super::GrokParserConfig;
    use crate::event::LogEvent;
    use crate::{
        event,
        test_util::runtime,
        topology::config::{TransformConfig, TransformContext},
        Event,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn parse_log(
        event: &str,
        pattern: &str,
        field: Option<&str>,
        drop_field: bool,
        types: &[(&str, &str)],
    ) -> LogEvent {
        let rt = runtime();
        let event = Event::from(event);
        let mut parser = GrokParserConfig {
            pattern: pattern.into(),
            field: field.map(|s| s.into()),
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
        }
        .build(TransformContext::new_test(rt.executor()))
        .unwrap();
        parser.transform(event).unwrap().into_log()
    }

    #[test]
    fn grok_parser_adds_parsed_fields_to_event() {
        let event = parse_log(
            r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
            "%{HTTPD_COMMONLOG}",
            None,
            true,
            &[],
        );

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

        assert_eq!(expected, serde_json::to_value(&event.all_fields()).unwrap());
    }

    #[test]
    fn grok_parser_does_nothing_on_no_match() {
        let event = parse_log(
            r#"help i'm stuck in an http server"#,
            "%{HTTPD_COMMONLOG}",
            None,
            true,
            &[],
        );

        assert_eq!(2, event.keys().count());
        assert_eq!(
            event::Value::from("help i'm stuck in an http server"),
            event[&event::log_schema().message_key()]
        );
        assert!(
            event[&event::log_schema().timestamp_key()]
                .to_string_lossy()
                .len()
                > 0
        );
    }

    #[test]
    fn grok_parser_can_not_drop_parsed_field() {
        let event = parse_log(
            r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
            "%{HTTPD_COMMONLOG}",
            None,
            false,
            &[],
        );

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

        assert_eq!(expected, serde_json::to_value(&event.all_fields()).unwrap());
    }

    #[test]
    fn grok_parser_does_nothing_on_missing_field() {
        let event = parse_log(
            "i am the only field",
            "^(?<foo>.*)",
            Some("bar"),
            false,
            &[],
        );

        assert_eq!(2, event.keys().count());
        assert_eq!(
            event::Value::from("i am the only field"),
            event[&event::log_schema().message_key()]
        );
        assert!(
            event[&event::log_schema().timestamp_key()]
                .to_string_lossy()
                .len()
                > 0
        );
    }

    #[test]
    fn grok_parser_coerces_types() {
        let event = parse_log(
            r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
            "%{HTTPD_COMMONLOG}",
            None,
            true,
            &[("response", "int"), ("bytes", "int")],
        );

        let expected = json!({
            "clientip": "109.184.11.34",
            "ident": "-",
            "auth": "-",
            "timestamp": "12/Dec/2015:18:32:56 +0100",
            "verb": "GET",
            "request": "/administrator/",
            "httpversion": "1.1",
            "rawrequest": "",
            "response": 200,
            "bytes": 4263,
        });

        assert_eq!(expected, serde_json::to_value(&event.all_fields()).unwrap());
    }

    #[test]
    fn grok_parser_does_not_drop_parsed_message_field() {
        let event = parse_log(
            "12/Dec/2015:18:32:56 +0100 42",
            "%{HTTPDATE:timestamp} %{NUMBER:message}",
            None,
            true,
            &[],
        );

        let expected = json!({
            "timestamp": "12/Dec/2015:18:32:56 +0100",
            "message": "42",
        });

        assert_eq!(expected, serde_json::to_value(&event.all_fields()).unwrap());
    }
}
