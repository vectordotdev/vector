use super::Transform;
use crate::serde::Fields;
use crate::{
    event::{Event, Value},
    internal_events::AddFieldsEventProcessed,
    template::Template,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use chrono::{DateTime, Utc};
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

pub struct AddFields {
    fields: IndexMap<Atom, TemplateOrValue>,
    overwrite: bool,
}

inventory::submit! {
    TransformDescription::new_without_default::<AddFieldsConfig>("add_fields")
}

#[typetag::serde(name = "add_fields")]
impl TransformConfig for AddFieldsConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(AddFields::new(
            self.fields
                .clone()
                .all_fields()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            self.overwrite,
        )))
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
    pub fn new(fields: IndexMap<Atom, TomlValue>, overwrite: bool) -> Self {
        let mut new_fields = IndexMap::new();

        for (k, v) in fields {
            flatten_field(k.into(), v, &mut new_fields);
        }

        AddFields {
            fields: new_fields,
            overwrite,
        }
    }
}

impl Transform for AddFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(AddFieldsEventProcessed);

        for (key, value_or_template) in self.fields.clone() {
            let value = match value_or_template {
                TemplateOrValue::Template(v) => match v.render_string(&event) {
                    Ok(v) => v,
                    Err(_) => {
                        warn!(
                            "Failed to render templated value at key `{}`, dropping.",
                            key
                        );
                        continue;
                    }
                }
                .into(),
                TemplateOrValue::Value(v) => v,
            };
            if self.overwrite {
                if let Some(_) = event.as_mut_log().insert(&key, value) {
                    debug!(
                        message = "Field overwritten",
                        field = key.as_ref(),
                        rate_limit_secs = 30,
                    )
                }
            } else {
                if event.as_mut_log().contains(&key) {
                    debug!(
                        message = "Field not overwritten",
                        field = key.as_ref(),
                        rate_limit_secs = 30,
                    )
                } else {
                    event.as_mut_log().insert(key, value);
                }
            }
        }

        Some(event)
    }
}

fn flatten_field(key: Atom, value: TomlValue, new_fields: &mut IndexMap<Atom, TemplateOrValue>) {
    match value {
        TomlValue::String(s) => match Template::try_from(s.as_str()) {
            Ok(t) => new_fields.insert(key, t.into()),
            Err(error) => {
                error!(message = "invalid template", %error);
                new_fields.insert(key, Value::from(s).into())
            }
        },
        TomlValue::Integer(i) => {
            let i = Value::from(i);
            new_fields.insert(key, i.into())
        }
        TomlValue::Float(f) => {
            let f = Value::from(f);
            new_fields.insert(key, f.into())
        }
        TomlValue::Boolean(b) => {
            let b = Value::from(b);
            new_fields.insert(key, b.into())
        }
        TomlValue::Datetime(dt) => {
            let dt = dt.to_string();
            if let Ok(ts) = dt.parse::<DateTime<Utc>>() {
                let ts = Value::from(ts);
                new_fields.insert(key, ts.into())
            } else {
                let dt = Value::from(dt);
                new_fields.insert(key, dt.into())
            }
        }
        TomlValue::Array(vals) => {
            for (i, val) in vals.into_iter().enumerate() {
                let key = format!("{}[{}]", key, i);
                flatten_field(key.into(), val, new_fields);
            }

            None
        }
        TomlValue::Table(map) => {
            for (table_key, value) in map {
                let key = format!("{}.{}", key, table_key);
                flatten_field(key.into(), value, new_fields);
            }

            None
        }
    };
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
        let mut augment = AddFields::new(fields, true);

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
        let mut augment = AddFields::new(fields, true);

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

        let mut augment = AddFields::new(fields, false);

        let new_event = augment.transform(event.clone()).unwrap();

        assert_eq!(new_event, event);
    }

    #[test]
    fn add_fields_preseves_types() {
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

        let mut transform = AddFields::new(fields, false);

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
