use super::Transform;
use crate::{
    event::{self, Event},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    types::{parse_check_conversion_map, Conversion},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SplitConfig {
    pub field_names: Vec<Atom>,
    pub separator: Option<String>,
    pub field: Option<Atom>,
    pub drop_field: bool,
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<SplitConfig>("split")
}

#[typetag::serde(name = "split")]
impl TransformConfig for SplitConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());

        let types = parse_check_conversion_map(&self.types, &self.field_names)
            .map_err(|err| format!("{}", err))?;

        // don't drop the source field if it's getting overwritten by a parsed value
        let drop_field = self.drop_field && !self.field_names.iter().any(|f| f == field);

        Ok(Box::new(Split::new(
            self.field_names.clone(),
            self.separator.clone(),
            field.clone(),
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
        "split"
    }
}

pub struct Split {
    field_names: Vec<(Atom, Conversion)>,
    separator: Option<String>,
    field: Atom,
    drop_field: bool,
}

impl Split {
    pub fn new(
        field_names: Vec<Atom>,
        separator: Option<String>,
        field: Atom,
        drop_field: bool,
        types: HashMap<Atom, Conversion>,
    ) -> Self {
        let field_names = field_names
            .into_iter()
            .map(|name| {
                let conversion = types.get(&name).unwrap_or(&Conversion::Bytes).clone();
                (name, conversion)
            })
            .collect();

        Self {
            field_names,
            separator,
            field,
            drop_field,
        }
    }
}

impl Transform for Split {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = &value {
            for ((name, conversion), value) in self
                .field_names
                .iter()
                .zip(split(value, self.separator.clone()).into_iter())
            {
                match conversion.convert(value.as_bytes().into()) {
                    Ok(value) => {
                        event.as_mut_log().insert(name.clone(), value);
                    }
                    Err(error) => {
                        debug!(
                            message = "Could not convert types.",
                            name = &name[..],
                            %error
                        );
                    }
                }
            }
            if self.drop_field {
                event.as_mut_log().remove(&self.field);
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
            );
        };

        Some(event)
    }
}

// Splits the given input by a separator.
// If the separator is `None`, then it will split on whitespace.
pub fn split(input: &str, separator: Option<String>) -> Vec<&str> {
    match separator {
        Some(separator) => input.split(&separator).collect(),
        None => input.split_whitespace().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::split;
    use super::SplitConfig;
    use crate::event::{LogEvent, Value};
    use crate::{
        test_util::runtime,
        topology::config::{TransformConfig, TransformContext},
        Event,
    };
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn split_whitespace() {
        assert_eq!(split("foo bar", None), &["foo", "bar"]);
        assert_eq!(split("foo\t bar", None), &["foo", "bar"]);
        assert_eq!(split("foo  \t bar     baz", None), &["foo", "bar", "baz"]);
    }

    #[test]
    fn split_comma() {
        assert_eq!(split("foo", Some(",".to_string())), &["foo"]);
        assert_eq!(split("foo,bar", Some(",".to_string())), &["foo", "bar"]);
    }

    #[test]
    fn split_semicolon() {
        assert_eq!(
            split("foo,bar;baz", Some(";".to_string())),
            &["foo,bar", "baz"]
        );
    }

    fn parse_log(
        text: &str,
        fields: &str,
        separator: Option<String>,
        field: Option<&str>,
        drop_field: bool,
        types: &[(&str, &str)],
    ) -> LogEvent {
        let rt = runtime();
        let event = Event::from(text);
        let field_names = fields.split(' ').map(|s| s.into()).collect::<Vec<Atom>>();
        let field = field.map(|f| f.into());
        let mut parser = SplitConfig {
            field_names,
            separator,
            field,
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
            ..Default::default()
        }
        .build(TransformContext::new_test(rt.executor()))
        .unwrap();

        parser.transform(event).unwrap().into_log()
    }

    #[test]
    fn split_adds_parsed_field_to_event() {
        let log = parse_log("1234 5678", "status time", None, None, false, &[]);

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn split_does_drop_parsed_field() {
        let log = parse_log("1234 5678", "status time", None, Some("message"), true, &[]);

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_none());
    }

    #[test]
    fn split_does_not_drop_same_name_parsed_field() {
        let log = parse_log(
            "1234 yes",
            "status message",
            None,
            Some("message"),
            true,
            &[],
        );

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
    }

    #[test]
    fn split_coerces_fields_to_types() {
        let log = parse_log(
            "1234 yes 42.3 word",
            "code flag number rest",
            None,
            None,
            false,
            &[("flag", "bool"), ("code", "integer"), ("number", "float")],
        );

        assert_eq!(log[&"number".into()], Value::Float(42.3));
        assert_eq!(log[&"flag".into()], Value::Boolean(true));
        assert_eq!(log[&"code".into()], Value::Integer(1234));
        assert_eq!(log[&"rest".into()], Value::Bytes("word".into()));
    }

    #[test]
    fn split_works_with_different_separator() {
        let log = parse_log(
            "1234,foo,bar",
            "code who why",
            Some(",".into()),
            None,
            false,
            &[("code", "integer"), ("who", "string"), ("why", "string")],
        );
        assert_eq!(log[&"code".into()], Value::Integer(1234));
        assert_eq!(log[&"who".into()], Value::Bytes("foo".into()));
        assert_eq!(log[&"why".into()], Value::Bytes("bar".into()));
    }
}
