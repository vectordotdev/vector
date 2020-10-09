use super::Transform;
use crate::event::Lookup;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    internal_events::{RemoveFieldsEventProcessed, RemoveFieldsFieldMissing},
    Event,
};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveFieldsConfig {
    fields: Vec<Lookup>,
    drop_empty: Option<bool>,
}

pub struct RemoveFields {
    fields: Vec<Lookup>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new::<RemoveFieldsConfig>("remove_fields")
}

impl GenerateConfig for RemoveFieldsConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "remove_fields")]
impl TransformConfig for RemoveFieldsConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(RemoveFields {
            fields: self.fields.clone(),
            drop_empty: self.drop_empty.unwrap_or(false),
        }))
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

impl RemoveFields {
    pub fn new(fields: Vec<Atom>, drop_empty: bool) -> crate::Result<Self> {
        let mut lookups = Vec::with_capacity(fields.len());
        for field in fields {
            let string = field.to_string(); // TODO: Step 6 of https://github.com/timberio/vector/blob/c4707947bd876a0ff7d7aa36717ae2b32b731593/rfcs/2020-05-25-more-usable-logevents.md#sales-pitch.
            lookups.push(Lookup::try_from(string)?);
        }
        Ok(RemoveFields {
            fields: lookups,
            drop_empty,
        })
    }
}

impl Transform for RemoveFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(RemoveFieldsEventProcessed);

        let log = event.as_mut_log();
        for field in &self.fields {
            let field_string = field.to_string();
            let old_val = log.remove_prune(&field_string, self.drop_empty);
            if old_val.is_none() {
                emit!(RemoveFieldsFieldMissing {
                    field: &field_string
                });
            }
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
        event.as_mut_log().insert("to_remove", "some value");
        event.as_mut_log().insert("to_keep", "another value");

        let mut transform =
            RemoveFields::new(vec!["to_remove".into(), "unknown".into()], false).unwrap();

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get(&"to_remove".into()).is_none());
        assert!(new_event.as_log().get(&"unknown".into()).is_none());
        assert_eq!(
            new_event.as_log()[&"to_keep".into()],
            "another value".into()
        );
    }
}
