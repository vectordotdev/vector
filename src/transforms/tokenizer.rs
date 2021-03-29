use crate::{
    config::{DataType, GlobalOptions, TransformConfig, TransformDescription},
    event::{Event, LookupBuf, PathComponent, PathIter, Value},
    internal_events::{TokenizerConvertFailed, TokenizerFieldMissing},
    transforms::{FunctionTransform, Transform},
    types::{parse_check_conversion_map, Conversion},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use shared::{tokenize::parse, TimeZone};
use std::collections::HashMap;
use std::str;

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct TokenizerConfig {
    pub field_names: Vec<LookupBuf>,
    pub field: Option<LookupBuf>,
    pub drop_field: bool,
    pub types: HashMap<LookupBuf, String>,
    pub timezone: Option<TimeZone>,
}

inventory::submit! {
    TransformDescription::new::<TokenizerConfig>("tokenizer")
}

impl_generate_config_from_default!(TokenizerConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "tokenizer")]
impl TransformConfig for TokenizerConfig {
    async fn build(&self, globals: &GlobalOptions) -> crate::Result<Transform> {
        let field = self
            .field
            .clone()
            .unwrap_or_else(|| crate::config::log_schema().message_key().clone());

        let timezone = self.timezone.unwrap_or(globals.timezone);

        let types = parse_check_conversion_map(
            &self
                .types
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            &self
                .field_names
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>(),
            timezone,
        )?
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();

        // don't drop the source field if it's getting overwritten by a parsed value
        let drop_field = self.drop_field && !self.field_names.iter().any(|f| **f == *field);

        Ok(Transform::function(Tokenizer::new(
            self.field_names.clone(),
            field,
            self.drop_field,
            types,
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "tokenizer"
    }
}

#[derive(Clone, Debug)]
pub struct Tokenizer {
    types: Vec<(LookupBuf, Conversion)>,
    field: LookupBuf,
    drop_field: bool,
}

impl Tokenizer {
    pub fn new(
        field_names: Vec<LookupBuf>,
        field: LookupBuf,
        drop_field: bool,
        types: HashMap<LookupBuf, Conversion>,
    ) -> Self {
        let types: Vec<_> = field_names
            .into_iter()
            .map(|name| {
                let conversion = types.get(&name).unwrap_or(&Conversion::Bytes).clone();
                (name, conversion)
            })
            .collect();

        // don't drop the source field if it's getting overwritten by a parsed value
        let drop_field = drop_field && !types.iter().any(|(f, _c)| *f == field);

        Self {
            field,
            drop_field,
            types,
        }
    }
}

impl FunctionTransform for Tokenizer {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        let value = event.as_log().get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = &value {
            for ((name, conversion), value) in self.types.iter().zip(parse(value).into_iter()) {
                match conversion.convert::<Value>(Bytes::copy_from_slice(value.as_bytes())) {
                    Ok(value) => {
                        event.as_mut_log().insert(name.clone(), value);
                    }
                    Err(error) => {
                        emit!(TokenizerConvertFailed {
                            field: &name,
                            error
                        });
                    }
                }
            }
            if self.drop_field {
                event.as_mut_log().remove(&self.field, false);
            }
        } else {
            emit!(TokenizerFieldMissing { field: &self.field });
        };

        output.push(event)
    }
}

#[cfg(test)]
mod tests {
    use super::TokenizerConfig;
    use crate::{
        config::{GlobalOptions, TransformConfig},
        event::{LogEvent, Lookup, Value},
        log_event, Event,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<TokenizerConfig>();
    }

    async fn parse_log(
        text: &str,
        fields: &str,
        field: Option<LookupBuf>,
        drop_field: bool,
        types: &[(&str, &str)],
    ) -> LogEvent {
        let event = log_event! {
            crate::config::log_schema().message_key().clone() => text.to_string(),
            crate::config::log_schema().timestamp_key().clone() => chrono::Utc::now(),
        };
        let field_names = fields
            .split(' ')
            .map(|s| LookupBuf::from_str(s).unwrap_or_else(|_| LookupBuf::from(s)))
            .collect::<Vec<_>>();
        let mut parser = TokenizerConfig {
            field_names,
            field,
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
    async fn tokenizer_adds_parsed_field_to_event() {
        let log = parse_log("1234 5678", "status time", None, false, &[]).await;

        assert_eq!(log[Lookup::from("status")], "1234".into());
        assert_eq!(log[Lookup::from("time")], "5678".into());
        assert!(log.get(Lookup::from("message")).is_some());
    }

    #[tokio::test]
    async fn tokenizer_does_drop_parsed_field() {
        let log = parse_log(
            "1234 5678",
            "status time",
            Some(LookupBuf::from("message")),
            true,
            &[],
        )
        .await;

        assert_eq!(log[Lookup::from("status")], "1234".into());
        assert_eq!(log[Lookup::from("time")], "5678".into());
        assert!(log.get(Lookup::from("message")).is_none());
    }

    #[tokio::test]
    async fn tokenizer_does_not_drop_same_name_parsed_field() {
        let log = parse_log(
            "1234 yes",
            "status message",
            Some(LookupBuf::from("message")),
            true,
            &[],
        )
        .await;

        assert_eq!(log[Lookup::from("status")], "1234".into());
        assert_eq!(log[Lookup::from("message")], "yes".into());
    }

    #[tokio::test]
    async fn tokenizer_coerces_fields_to_types() {
        let log = parse_log(
            "1234 yes 42.3 word",
            "code flag number rest",
            None,
            false,
            &[("flag", "bool"), ("code", "integer"), ("number", "float")],
        )
        .await;

        assert_eq!(log[Lookup::from("number")], Value::Float(42.3));
        assert_eq!(log[Lookup::from("flag")], Value::Boolean(true));
        assert_eq!(log[Lookup::from("code")], Value::Integer(1234));
        assert_eq!(log[Lookup::from("rest")], Value::Bytes("word".into()));
    }

    #[tokio::test]
    async fn tokenizer_keeps_dash_as_dash() {
        let log = parse_log(
            "1234 - foo",
            "code who why",
            None,
            false,
            &[("code", "integer"), ("who", "string"), ("why", "string")],
        )
        .await;
        assert_eq!(log[Lookup::from("code")], Value::Integer(1234));
        assert_eq!(log[Lookup::from("who")], Value::Bytes("-".into()));
        assert_eq!(log[Lookup::from("why")], Value::Bytes("foo".into()));
    }
}
