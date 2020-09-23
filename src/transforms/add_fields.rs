use super::Transform;
use crate::serde::Fields;
use crate::{
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
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
use std::convert::TryFrom;
use string_cache::DefaultAtom as Atom;
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
    TransformDescription::new_without_default::<AddFieldsConfig>("add_fields")
}

#[typetag::serde(name = "add_fields")]
impl TransformConfig for AddFieldsConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(AddFields::new(
            self.fields.clone().all_fields().collect(),
            self.overwrite,
        )?))
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
    pub fn new(mut fields: IndexMap<Atom, TomlValue>, overwrite: bool) -> crate::Result<Self> {
        let mut set =
            Lookup::from_indexmap(fields.drain(..).map(|(k, v)| (k.to_string(), v)).collect())?;
        let mut with_templates = IndexMap::with_capacity(set.len());
        for (k, v) in set.drain(..) {
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
    use super::AddFields;
    use crate::{event::Event, transforms::Transform};
    use indexmap::IndexMap;
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn add_fields_event() {
        let event = Event::from("augment me");
        let mut fields = IndexMap::new();
        fields.insert("some_key".into(), "some_val".into());
        let mut augment = AddFields::new(fields, true).unwrap();

        let new_event = augment.transform(event).unwrap();

        let key = Atom::from("some_key".to_string());
        let kv = new_event.as_log().get(&key);

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

        let key = Atom::from("some_key".to_string());
        let kv = new_event.as_log().get(&key);

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
        let event = Event::from("hello world");

        let mut fields = IndexMap::new();
        fields.insert("float".into(), 4.5.into());
        fields.insert("int".into(), 4.into());
        fields.insert("string".into(), "thisisastring".into());
        fields.insert("bool".into(), true.into());
        fields.insert("array".into(), vec![1, 2, 3].into());

        let mut map = HashMap::new();
        map.insert("key", "value");

        fields.insert("table".into(), map.into());

        let mut transform = AddFields::new(fields, false).unwrap();

        let event = transform.transform(event).unwrap().into_log();

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
