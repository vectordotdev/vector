use super::Transform;
use crate::{
    event::{Event, ValueKind},
    topology::config::{DataType, TransformConfig},
};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;
use toml::value::Value;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddFieldsConfig {
    pub fields: IndexMap<String, Value>,
}

pub struct AddFields {
    fields: IndexMap<Atom, ValueKind>,
}

#[typetag::serde(name = "augmenter")]
impl TransformConfig for AddFieldsConfig {
    fn build(&self) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(AddFields::new(self.fields.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

impl AddFields {
    pub fn new(fields: IndexMap<String, Value>) -> Self {
        let mut new_fields = IndexMap::new();

        for (k, v) in fields {
            flatten_field(k.into(), v, &mut new_fields);
        }

        AddFields { fields: new_fields }
    }
}

impl Transform for AddFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        for (key, value) in self.fields.clone() {
            event.as_mut_log().insert_explicit(key, value.into());
        }

        Some(event)
    }
}

fn flatten_field(key: Atom, value: Value, new_fields: &mut IndexMap<Atom, ValueKind>) {
    match value {
        Value::String(s) => new_fields.insert(key, s.into()),
        Value::Integer(i) => new_fields.insert(key, i.into()),
        Value::Float(f) => new_fields.insert(key, f.into()),
        Value::Boolean(b) => new_fields.insert(key, b.into()),
        Value::Datetime(dt) => {
            let dt = dt.to_string();
            if let Ok(ts) = dt.parse::<DateTime<Utc>>() {
                new_fields.insert(key, ts.into())
            } else {
                new_fields.insert(key, dt.into())
            }
        }
        Value::Array(vals) => {
            for (i, val) in vals.into_iter().enumerate() {
                let key = format!("{}[{}]", key, i);
                flatten_field(key.into(), val, new_fields);
            }

            None
        }
        Value::Table(map) => {
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
        let mut augment = AddFields::new(fields);

        let new_event = augment.transform(event).unwrap();

        let key = Atom::from("some_key".to_string());
        let kv = new_event.as_log().get(&key);

        let val = "some_val".to_string();
        assert_eq!(kv, Some(&val.into()));
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

        let mut transform = AddFields::new(fields);

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
