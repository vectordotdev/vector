use super::Transform;
use crate::serde::Fields;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Lookup,
    event::{Event, Value},
    internal_events::{
        AddFieldsEventProcessed, AddFieldsFieldNotOverwritten, AddFieldsFieldOverwritten,
        AddFieldsTemplateRenderingError,
    },
    template::Template,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, str::FromStr};
use toml::value::Value as TomlValue;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddFieldsConfig {
    pub fields: Fields<TomlValue>,
    #[serde(default = "crate::serde::default_true")]
    pub overwrite: bool,
}

#[derive(Clone)]
enum TemplateOrValue {
    Template(Template),
    Value(Value),
}

impl From<Template> for TemplateOrValue {
    fn from(v: Template) -> Self {
        TemplateOrValue::Template(v)
    }
}

impl From<Value> for TemplateOrValue {
    fn from(v: Value) -> Self {
        TemplateOrValue::Value(v)
    }
}

#[derive(Clone)]
pub struct AddFields {
    fields: IndexMap<Lookup, TemplateOrValue>,
    overwrite: bool,
}

inventory::submit! {
    TransformDescription::new::<AddFieldsConfig>("add_fields")
}

impl GenerateConfig for AddFieldsConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "add_fields")]
impl TransformConfig for AddFieldsConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let all_fields = self.fields.clone().all_fields().collect::<IndexMap<_, _>>();
        let mut fields = IndexMap::with_capacity(all_fields.len());
        for (key, value) in all_fields {
            fields.insert(Lookup::from_str(&key)?, Value::try_from(value)?);
        }
        Ok(Box::new(AddFields::new(fields, self.overwrite)?))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "add_fields"
    }
}

impl AddFields {
    pub fn new(mut fields: IndexMap<Lookup, Value>, overwrite: bool) -> crate::Result<Self> {
        let mut with_templates = IndexMap::with_capacity(fields.len());
        for (k, v) in fields.drain(..) {
            let maybe_template = match v {
                Value::Bytes(s) => match Template::try_from(String::from_utf8(s.to_vec())?) {
                    Ok(t) => TemplateOrValue::from(t),
                    Err(_) => TemplateOrValue::from(Value::Bytes(s)),
                },
                v => TemplateOrValue::from(v),
            };
            with_templates.insert(k, maybe_template);
        }

        Ok(AddFields {
            fields: with_templates,
            overwrite,
        })
    }
}

impl Transform for AddFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(AddFieldsEventProcessed);

        for (key, value_or_template) in self.fields.clone() {
            let key_string = key.to_string(); // TODO: Step 6 of https://github.com/timberio/vector/blob/c4707947bd876a0ff7d7aa36717ae2b32b731593/rfcs/2020-05-25-more-usable-logevents.md#sales-pitch.
            let value = match value_or_template {
                TemplateOrValue::Template(v) => match v.render_string(&event) {
                    Ok(v) => v,
                    Err(_) => {
                        emit!(AddFieldsTemplateRenderingError {
                            field: &format!("{}", &key),
                        });
                        continue;
                    }
                }
                .into(),
                TemplateOrValue::Value(v) => v,
            };
            if self.overwrite {
                if event.as_mut_log().insert(&key_string, value).is_some() {
                    emit!(AddFieldsFieldOverwritten {
                        field: &format!("{}", &key),
                    });
                }
            } else if event.as_mut_log().contains(&key_string) {
                emit!(AddFieldsFieldNotOverwritten {
                    field: &format!("{}", &key),
                });
            } else {
                event.as_mut_log().insert(&key_string, value);
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{iter::FromIterator, string::ToString};

    #[test]
    fn add_fields_event() {
        let event = Event::from("augment me");
        let mut fields = IndexMap::new();
        fields.insert("some_key".into(), "some_val".into());
        let mut augment = AddFields::new(fields, true).unwrap();

        let new_event = augment.transform(event).unwrap();

        let key = Lookup::from_str("some_key").unwrap().to_string();
        let kv = new_event.as_log().get_flat(&key);

        let val = "some_val".to_string();
        assert_eq!(kv, Some(&val.into()));
    }

    #[test]
    fn add_fields_templating() {
        let event = Event::from("augment me");
        let mut fields = IndexMap::new();
        fields.insert("some_key".into(), "{{message}} {{message}}".into());
        let mut augment = AddFields::new(fields, true).unwrap();

        let new_event = augment.transform(event).unwrap();

        let key = Lookup::from_str("some_key").unwrap().to_string();
        let kv = new_event.as_log().get_flat(&key);

        let val = "augment me augment me".to_string();
        assert_eq!(kv, Some(&val.into()));
    }

    #[test]
    fn add_fields_overwrite() {
        let mut event = Event::from("");
        event.as_mut_log().insert("some_key", "some_message");

        let mut fields = IndexMap::new();
        fields.insert("some_key".into(), "some_overwritten_message".into());

        let mut augment = AddFields::new(fields, false).unwrap();

        let new_event = augment.transform(event.clone()).unwrap();

        assert_eq!(new_event, event);
    }

    #[test]
    fn add_fields_preserves_types() {
        crate::test_util::trace_init();
        let event = Event::from("hello world");

        let mut fields = IndexMap::new();
        fields.insert(Lookup::from_str("float").unwrap(), Value::from(4.5));
        fields.insert(Lookup::from_str("int").unwrap(), Value::from(4));
        fields.insert(
            Lookup::from_str("string").unwrap(),
            Value::from("thisisastring"),
        );
        fields.insert(Lookup::from_str("bool").unwrap(), Value::from(true));
        fields.insert(
            Lookup::from_str("array").unwrap(),
            Value::from(vec![1_isize, 2, 3]),
        );

        let mut map = IndexMap::new();
        map.insert(String::from("key"), Value::from("value"));

        fields.insert(Lookup::from_str("table").unwrap(), Value::from_iter(map));

        let mut transform = AddFields::new(fields, false).unwrap();

        let event = transform.transform(event).unwrap().into_log();

        tracing::error!(?event);
        assert_eq!(event[&"float".into()], 4.5.into());
        assert_eq!(event[&"int".into()], 4.into());
        assert_eq!(event[&"string".into()], "thisisastring".into());
        assert_eq!(event[&"bool".into()], true.into());
        assert_eq!(event[&"array[0]".into()], 1.into());
        assert_eq!(event[&"array[1]".into()], 2.into());
        assert_eq!(event[&"array[2]".into()], 3.into());
        assert_eq!(event[&"table.key".into()], "value".into());
    }
}
