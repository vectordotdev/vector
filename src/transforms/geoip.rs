use super::Transform;
use crate::{
    event::{Event, ValueKind},
    topology::config::{DataType, TransformConfig},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;
use toml::value::Value;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GeoipConfig {
    pub source: Atom,
    pub database: String,
    pub target: String,
}

pub struct Geoip {
    pub data: IndexMap<String, String>,
}

#[typetag::serde(name = "geoip")]
impl TransformConfig for GeoipConfig {
    fn build(&self) -> Result<Box<dyn Transform>, crate::Error> {
        Ok(Box::new(Geoip::new(
            self.source.clone(),
            self.database.clone(),
            self.target.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

impl Geoip {
    pub fn new(source: Atom, database: String, target: String) -> Self {
        let mut geo_data = IndexMap::new();
        let k = String::from("foo");
        let v = String::from("bar");

        geo_data.insert(k, v);
        Geoip { data: geo_data }
    }
}

impl Transform for Geoip {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        for (key, value) in self.data.clone() {
            event
                .as_mut_log()
                .insert_explicit(Atom::from(key), value.into());
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::Geoip;
    use crate::{event::Event, transforms::Transform};
    use indexmap::IndexMap;
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn geoip_event() {
        let event = Event::from("augment me");
        let mut augment = Geoip::new(Atom::from("source"), "path/to/db".to_string(), "geoip".to_string());

        let new_event = augment.transform(event).unwrap();

        let key = Atom::from("foo".to_string());
        let kv = new_event.as_log().get(&key);

        let val = "bar".to_string();
        assert_eq!(kv, Some(&val.into()));
    }
}
