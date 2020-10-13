use super::util::tokenize::parse;
use super::Transform;
use crate::{
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
    event::{Event, PathComponent, PathIter, Value},
    internal_events::{TokenizerConvertFailed, TokenizerEventProcessed, TokenizerFieldMissing},
    types::{parse_check_conversion_map, Conversion},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct TokenizerConfig {
    pub field_names: Vec<Atom>,
    pub field: Option<Atom>,
    pub drop_field: bool,
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<TokenizerConfig>("tokenizer")
}

impl_generate_config_from_default!(TokenizerConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "tokenizer")]
impl TransformConfig for TokenizerConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .clone()
            .unwrap_or_else(|| Atom::from(crate::config::log_schema().message_key()));

        let types = parse_check_conversion_map(&self.types, &self.field_names)?;

        // don't drop the source field if it's getting overwritten by a parsed value
        let drop_field = self.drop_field && !self.field_names.iter().any(|f| **f == *field);

        Ok(Box::new(Tokenizer::new(
            self.field_names.clone(),
            field,
            drop_field,
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

pub struct Tokenizer {
    field_names: Vec<(Atom, Vec<PathComponent>, Conversion)>,
    field: Atom,
    drop_field: bool,
}

impl Tokenizer {
    pub fn new(
        field_names: Vec<Atom>,
        field: Atom,
        drop_field: bool,
        types: HashMap<Atom, Conversion>,
    ) -> Self {
        let field_names = field_names
            .into_iter()
            .map(|name| {
                let conversion = types.get(&name).unwrap_or(&Conversion::Bytes).clone();
                let path: Vec<PathComponent> = PathIter::new(&name).collect();
                (name, path, conversion)
            })
            .collect();

        Self {
            field_names,
            field,
            drop_field,
        }
    }
}

impl Transform for Tokenizer {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = &value {
            for ((name, path, conversion), value) in
                self.field_names.iter().zip(parse(value).into_iter())
            {
                match conversion.convert(Value::from(value.to_owned())) {
                    Ok(value) => {
                        event.as_mut_log().insert_path(path.clone(), value);
                    }
                    Err(error) => {
                        emit!(TokenizerConvertFailed { field: name, error });
                    }
                }
            }
            if self.drop_field {
                event.as_mut_log().remove(&self.field);
            }
        } else {
            emit!(TokenizerFieldMissing { field: &self.field });
        };

        emit!(TokenizerEventProcessed);

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::TokenizerConfig;
    use crate::event::{LogEvent, Value};
    use crate::{
        config::{TransformConfig, TransformContext},
        Event,
    };
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<TokenizerConfig>();
    }

    async fn parse_log(
        text: &str,
        fields: &str,
        field: Option<&str>,
        drop_field: bool,
        types: &[(&str, &str)],
    ) -> LogEvent {
        let event = Event::from(text);
        let field_names = fields.split(' ').map(|s| s.into()).collect::<Vec<Atom>>();
        let field = field.map(|f| f.into());
        let mut parser = TokenizerConfig {
            field_names,
            field,
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
        }
        .build(TransformContext::new_test())
        .await
        .unwrap();

        parser.transform(event).unwrap().into_log()
    }

    #[tokio::test]
    async fn tokenizer_adds_parsed_field_to_event() {
        let log = parse_log("1234 5678", "status time", None, false, &[]).await;

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[tokio::test]
    async fn tokenizer_does_drop_parsed_field() {
        let log = parse_log("1234 5678", "status time", Some("message"), true, &[]).await;

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_none());
    }

    #[tokio::test]
    async fn tokenizer_does_not_drop_same_name_parsed_field() {
        let log = parse_log("1234 yes", "status message", Some("message"), true, &[]).await;

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
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

        assert_eq!(log[&"number".into()], Value::Float(42.3));
        assert_eq!(log[&"flag".into()], Value::Boolean(true));
        assert_eq!(log[&"code".into()], Value::Integer(1234));
        assert_eq!(log[&"rest".into()], Value::Bytes("word".into()));
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
        assert_eq!(log[&"code".into()], Value::Integer(1234));
        assert_eq!(log[&"who".into()], Value::Bytes("-".into()));
        assert_eq!(log[&"why".into()], Value::Bytes("foo".into()));
    }
}
