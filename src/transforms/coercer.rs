use super::Transform;
use crate::event::Event;
use crate::runtime::TaskExecutor;
use crate::topology::config::{DataType, TransformConfig, TransformDescription};
use crate::types::{parse_conversion_map, Conversion};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct CoercerConfig {
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<CoercerConfig>("coercer")
}

#[typetag::serde(name = "coercer")]
impl TransformConfig for CoercerConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        let types = parse_conversion_map(&self.types)?;
        Ok(Box::new(Coercer { types }))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "coercer"
    }
}

pub struct Coercer {
    types: HashMap<Atom, Conversion>,
}

impl Transform for Coercer {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut log = event.into_log();
        for (field, conv) in &self.types {
            if let Some(value) = log.remove(field) {
                match conv.convert(value) {
                    Ok(converted) => log.insert_explicit(field, converted),
                    Err(error) => {
                        warn!(
                            message = "Could not convert types.",
                            field = &field[..],
                            %error,
                            rate_limit_secs = 10,
                        );
                    }
                }
            }
        }
        Some(Event::Log(log))
    }
}

#[cfg(test)]
mod tests {
    use super::CoercerConfig;
    use crate::event::{LogEvent, ValueKind};
    use crate::{topology::config::TransformConfig, Event};
    use pretty_assertions::assert_eq;

    fn parse_it() -> LogEvent {
        let rt = crate::runtime::Runtime::single_threaded().unwrap();
        let mut event = Event::from("dummy message");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("other", "no"),
            ("float", "broken"),
        ] {
            event.as_mut_log().insert_explicit(key, value);
        }

        let mut coercer = toml::from_str::<CoercerConfig>(
            r#"
            [types]
            number = "int"
            float = "float"
            bool = "bool"
            "#,
        )
        .unwrap()
        .build(rt.executor())
        .unwrap();
        coercer.transform(event).unwrap().into_log()
    }

    #[test]
    fn coercer_converts_valid_fields() {
        let log = parse_it();
        assert_eq!(log[&"number".into()], ValueKind::Integer(1234));
        assert_eq!(log[&"bool".into()], ValueKind::Boolean(true));
    }

    #[test]
    fn coercer_leaves_unnamed_fields_as_is() {
        let log = parse_it();
        assert_eq!(log[&"other".into()], ValueKind::Bytes("no".into()));
    }

    #[test]
    fn coercer_drops_nonconvertible_fields() {
        let log = parse_it();
        assert!(log.get(&"float".into()).is_none());
    }
}
