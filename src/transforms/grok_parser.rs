use crate::{
    config::{log_schema, DataType, GlobalOptions, TransformConfig, TransformDescription},
    event::{Event, LookupBuf, PathComponent, PathIter, Value},
    internal_events::{GrokParserConversionFailed, GrokParserFailedMatch, GrokParserMissingField},
    transforms::{FunctionTransform, Transform},
    types::{parse_conversion_map, Conversion},
};
use bytes::Bytes;
use grok::Pattern;
use serde::{Deserialize, Serialize};
use shared::TimeZone;
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::str;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid grok pattern: {}", source))]
    InvalidGrok { source: grok::Error },
}

#[derive(Deserialize, Serialize, Debug, Derivative, Clone)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct GrokParserConfig {
    pub pattern: String,
    pub field: Option<LookupBuf>,
    #[derivative(Default(value = "true"))]
    pub drop_field: bool,
    pub types: HashMap<LookupBuf, String>,
    pub timezone: Option<TimeZone>,
}

inventory::submit! {
    TransformDescription::new::<GrokParserConfig>("grok_parser")
}

impl_generate_config_from_default!(GrokParserConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "grok_parser")]
impl TransformConfig for GrokParserConfig {
    async fn build(&self, globals: &GlobalOptions) -> crate::Result<Transform> {
        let field = self
            .field
            .clone()
            .unwrap_or_else(|| log_schema().message_key().clone());

        let mut grok = grok::Grok::with_patterns();

        let timezone = self.timezone.unwrap_or(globals.timezone);
        let types = parse_conversion_map(
            &self
                .types
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )?
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();

        Ok(grok
            .compile(&self.pattern, true)
            .map(|p| GrokParser {
                pattern: self.pattern.clone(),
                pattern_built: p,
                field: field.clone(),
                drop_field: self.drop_field,
                types,
                paths: HashMap::new(),
            })
            .map(Transform::function)
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct GrokParser {
    #[derivative(Debug = "ignore")]
    pattern_built: Pattern,
    pattern: String,
    field: LookupBuf,
    drop_field: bool,
    types: HashMap<LookupBuf, Conversion>,
    paths: HashMap<String, LookupBuf>,
}

impl Clone for GrokParser {
    fn clone(&self) -> Self {
        Self {
            pattern_built: grok::Grok::with_patterns().compile(&self.pattern, true)
                .expect("Panicked while cloning an already valid Grok parser. For some reason, the pattern could not be built again."),
            pattern: self.pattern.clone(),
            field: self.field.clone(),
            drop_field: self.drop_field,
            types: self.types.clone(),
            paths: self.paths.clone(),
        }
    }
}

impl FunctionTransform for GrokParser {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let mut event = event.into_log();
        let value = event.get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = value {
            if let Some(matches) = self.pattern_built.match_against(&value) {
                let drop_field = self.drop_field && matches.get(&self.field.to_string()).is_none();
                for (name, value) in matches.iter() {
                    let name_lookup =
                        LookupBuf::from_str(name).unwrap_or_else(|_| LookupBuf::from(name));
                    let conv = self.types.get(&name_lookup).unwrap_or(&Conversion::Bytes);
                    match conv.convert::<Value>(Bytes::copy_from_slice(value.as_bytes())) {
                        Ok(value) => {
                            if let Some(path) = self.paths.get(name) {
                                event.insert(path.clone(), value);
                            } else {
                                event.insert(name_lookup, value);
                            }
                        }
                        Err(error) => emit!(GrokParserConversionFailed { name, error }),
                    }
                }

                if drop_field {
                    event.remove(&self.field, false);
                }
            } else {
                emit!(GrokParserFailedMatch {
                    value: value.as_ref(),
                });
            }
        } else {
            emit!(GrokParserMissingField { field: &self.field });
        }

        output.push(Event::Log(event));
    }
}

#[cfg(test)]
mod tests {
    use super::GrokParserConfig;
    use crate::{
        config::{log_schema, GlobalOptions, TransformConfig},
        event,
        event::{LogEvent, Value},
        log_event, Event,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GrokParserConfig>();
    }

    async fn parse_log(
        event: &str,
        pattern: &str,
        field: Option<&str>,
        drop_field: bool,
        types: &[(&str, &str)],
    ) -> LogEvent {
        let event = log_event! {
            log_schema().message_key().clone() => event.to_string(),
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
        };
        let mut parser = GrokParserConfig {
            pattern: pattern.into(),
            field: field.map(|s| s.into()),
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
            timezone: Default::default(),
        }
        .build(&GlobalOptions::default())
        .await
        .unwrap();
        let parser = parser.as_function();

        parser.transform_one(event).unwrap().into_log()
    }

    #[tokio::test]
    async fn grok_parser_adds_parsed_fields_to_event() {
        let event = parse_log(
            r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
            "%{HTTPD_COMMONLOG}",
            None,
            true,
            &[],
        ).await;

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

        assert_eq!(expected, serde_json::to_value(&event).unwrap());
    }

    #[tokio::test]
    async fn grok_parser_does_nothing_on_no_match() {
        let event = parse_log(
            r#"Help I'm stuck in an HTTP server"#,
            "%{HTTPD_COMMONLOG}",
            None,
            true,
            &[],
        )
        .await;

        assert_eq!(2, event.keys(true).count());
        assert_eq!(
            Value::from("Help I'm stuck in an HTTP server".to_string()),
            event[log_schema().message_key()]
        );
        assert!(!event[log_schema().timestamp_key()]
            .to_string_lossy()
            .is_empty());
    }

    #[tokio::test]
    async fn grok_parser_can_not_drop_parsed_field() {
        let event = parse_log(
            r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
            "%{HTTPD_COMMONLOG}",
            None,
            false,
            &[],
        ).await;

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

        assert_eq!(expected, serde_json::to_value(&event).unwrap());
    }

    #[tokio::test]
    async fn grok_parser_does_nothing_on_missing_field() {
        let event = parse_log(
            "i am the only field",
            "^(?<foo>.*)",
            Some("bar"),
            false,
            &[],
        )
        .await;

        assert_eq!(2, event.keys(true).count());
        assert_eq!(
            Value::from("i am the only field"),
            event[log_schema().message_key()]
        );
        assert!(!event[log_schema().timestamp_key()]
            .to_string_lossy()
            .is_empty());
    }

    #[tokio::test]
    async fn grok_parser_coerces_types() {
        let event = parse_log(
            r#"109.184.11.34 - - [12/Dec/2015:18:32:56 +0100] "GET /administrator/ HTTP/1.1" 200 4263"#,
            "%{HTTPD_COMMONLOG}",
            None,
            true,
            &[("response", "int"), ("bytes", "int")],
        ).await;

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

        assert_eq!(expected, serde_json::to_value(&event).unwrap());
    }

    #[tokio::test]
    async fn grok_parser_does_not_drop_parsed_message_field() {
        let event = parse_log(
            "12/Dec/2015:18:32:56 +0100 42",
            "%{HTTPDATE:timestamp} %{NUMBER:message}",
            None,
            true,
            &[],
        )
        .await;

        let expected = json!({
            "timestamp": "12/Dec/2015:18:32:56 +0100",
            "message": "42",
        });

        assert_eq!(expected, serde_json::to_value(&event).unwrap());
    }
}
