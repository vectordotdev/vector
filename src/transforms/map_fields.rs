use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MapFieldsConfig {
    pub fields: IndexMap<String, String>,
}

#[typetag::serde(name = "fieldmapper")]
impl TransformConfig for MapFieldsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(MapFields::new(self.fields.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

pub struct MapFields {
    fields: IndexMap<Atom, String>,
}

impl Transform for MapFields {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let old_log = event.into_log();
        let mut new_log = Event::new_empty_log();
        for (field, conv) in &self.fields {
            if let Some(value) = old_log.get(field) {
                new_log
                    .as_mut_log()
                    .insert_explicit(Atom::from(conv.as_str()), value.clone());
            }
        }
        Some(new_log)
    }
}

impl MapFields {
    pub fn new(fields: IndexMap<String, String>) -> Self {
        let mut new_fields = IndexMap::new();

        for (k, v) in fields {
            new_fields.insert(Atom::from(k), v.into());
        }

        MapFields { fields: new_fields }
    }
}

#[cfg(test)]
mod tests {
    use super::{MapFields, MapFieldsConfig};
    use crate::topology::config::TransformConfig;
    use crate::{event::Event, transforms::Transform};
    use indexmap::IndexMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn map_fields_event() {
        let mut event = Event::from("dummy message");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("other", "no"),
            ("float", "broken"),
        ] {
            event.as_mut_log().insert_explicit(key.into(), value.into());
        }

        let mut fields = IndexMap::new();
        fields.insert("number".into(), "number_1".into());
        let mut mapper = MapFields::new(fields);

        let new_event = mapper.transform(event).unwrap();

        let key = Atom::from("number_1".to_string());
        let kv = new_event.as_log().get(&key);

        let val = "1234".to_string();
        assert_eq!(kv, Some(&val.into()));
    }

    #[test]
    fn map_fields_event_from_config() {
        let mut mapper = toml::from_str::<MapFieldsConfig>(
            r#"
            [fields]
            number = "number_1"
            "#,
        )
        .unwrap()
        .build()
        .unwrap();

        let mut event = Event::from("dummy message");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("other", "no"),
            ("float", "broken"),
        ] {
            event.as_mut_log().insert_explicit(key.into(), value.into());
        }

        let new_event = mapper.transform(event).unwrap();

        let key = Atom::from("number_1".to_string());
        let kv = new_event.as_log().get(&key);

        let val = "1234".to_string();
        assert_eq!(kv, Some(&val.into()));
    }
}
