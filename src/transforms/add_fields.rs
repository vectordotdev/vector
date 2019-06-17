use super::Transform;
use crate::Event;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddFieldsConfig {
    pub fields: IndexMap<String, String>,
}

pub struct AddFields {
    fields: IndexMap<Atom, String>,
}

#[typetag::serde(name = "augmenter")]
impl crate::topology::config::TransformConfig for AddFieldsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(AddFields::new(self.fields.clone())))
    }
}

impl AddFields {
    pub fn new(fields: IndexMap<String, String>) -> Self {
        let fields = fields.into_iter().map(|(k, v)| (k.into(), v)).collect();

        AddFields { fields }
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

#[cfg(test)]
mod tests {
    use super::AddFields;
    use crate::{event::Event, transforms::Transform};
    use indexmap::IndexMap;
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
}
