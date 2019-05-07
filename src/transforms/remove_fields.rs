use super::Transform;
use crate::Event;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveFieldsConfig {
    pub fields: Vec<Atom>,
}

pub struct RemoveFields {
    fields: Vec<Atom>,
}

#[typetag::serde(name = "remove_fields")]
impl crate::topology::config::TransformConfig for RemoveFieldsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(RemoveFields::new(self.fields.clone())))
    }
}

impl RemoveFields {
    pub fn new(fields: Vec<Atom>) -> Self {
        RemoveFields { fields }
    }
}

impl Transform for RemoveFields {
    fn transform(&self, mut record: Event) -> Option<Event> {
        for field in &self.fields {
            record.as_mut_log().remove(field);
        }

        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::RemoveFields;
    use crate::{event::Event, transforms::Transform};

    #[test]
    fn remove_fields() {
        let mut record = Event::from("message");
        record
            .as_mut_log()
            .insert_explicit("to_remove".into(), "some value".into());
        record
            .as_mut_log()
            .insert_explicit("to_keep".into(), "another value".into());

        let transform = RemoveFields::new(vec!["to_remove".into(), "unknown".into()]);

        let new_record = transform.transform(record).unwrap();

        assert!(new_record.as_log().get(&"to_remove".into()).is_none());
        assert!(new_record.as_log().get(&"unknown".into()).is_none());
        assert_eq!(new_record[&"to_keep".into()], "another value".into());
    }
}
