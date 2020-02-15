use super::Transform;
use crate::{
    runtime::TaskExecutor,
    topology::config::{DataType, TransformConfig, TransformDescription},
    Event,
};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RenameFieldsConfig {
    pub fields: IndexMap<String, String>,
}

pub struct RenameFields {
    fields: IndexMap<Atom, Atom>,
}

inventory::submit! {
    TransformDescription::new_without_default::<RenameFieldsConfig>("remove_fields")
}

#[typetag::serde(name = "remove_fields")]
impl TransformConfig for RenameFieldsConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(RenameFields::new(self.fields.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "remove_fields"
    }
}

impl RenameFields {
    pub fn new(fields: IndexMap<String, String>) -> Self {
        RenameFields {
            fields: fields
                .into_iter()
                .map(|(k, v)| (Atom::from(k), Atom::from(v)))
                .collect(),
        }
    }
}

impl Transform for RenameFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        for (old_key, new_key) in &self.fields {
            let log = event.as_mut_log();
            if let Some(v) = log.remove(old_key) {
                log.insert(new_key, v)
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RenameFields;
    use crate::{event::Event, transforms::Transform};
    use indexmap::map::IndexMap;

    #[test]
    fn rename_fields() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("to_move", "some value");
        event.as_mut_log().insert("do_not_move", "not moved");
        let mut fields = IndexMap::new();
        fields.insert("to_move".into(), "moved".into());
        fields.insert("not_present".into(), "should_not_exist".into());

        let mut transform = RenameFields::new(fields);

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get(&"to_move".into()).is_none());
        assert_eq!(new_event.as_log()[&"moved".into()], "some value".into());
        assert!(new_event.as_log().get(&"not_present".into()).is_none());
        assert!(new_event.as_log().get(&"should_not_exist".into()).is_none());
        assert_eq!(
            new_event.as_log()[&"do_not_move".into()],
            "not moved".into()
        );
    }
}
