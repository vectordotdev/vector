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
    fn transform(&self, mut event: Event) -> Option<Event> {
        for field in &self.fields {
            event.as_mut_log().remove(field);
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RemoveFields;
    use crate::{event::Event, transforms::Transform};

    #[test]
    fn remove_fields() {
        let mut event = Event::from("message");
        event
            .as_mut_log()
            .insert_explicit("to_remove".into(), "some value".into());
        event
            .as_mut_log()
            .insert_explicit("to_keep".into(), "another value".into());

        let transform = RemoveFields::new(vec!["to_remove".into(), "unknown".into()]);

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get(&"to_remove".into()).is_none());
        assert!(new_event.as_log().get(&"unknown".into()).is_none());
        assert_eq!(new_event[&"to_keep".into()], "another value".into());
    }
}
