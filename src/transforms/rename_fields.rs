use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    event::Lookup,
    internal_events::{
        RenameFieldsEventProcessed, RenameFieldsFieldDoesNotExist, RenameFieldsFieldOverwritten,
    },
    serde::Fields,
};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RenameFieldsConfig {
    pub fields: Fields<Lookup>,
    drop_empty: Option<bool>,
}

pub struct RenameFields {
    fields: IndexMap<Lookup, Lookup>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new::<RenameFieldsConfig>("rename_fields")
}

impl GenerateConfig for RenameFieldsConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "rename_fields")]
impl TransformConfig for RenameFieldsConfig {
    async fn build(&self, _exec: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let mut fields = IndexMap::default();
        for (key, value) in self.fields.clone().all_fields() {
            fields.insert(
                key.to_string().parse::<Lookup>()?,
                value.to_string().parse::<Lookup>()?,
            );
        }
        Ok(Box::new(RenameFields::new(
            fields,
            self.drop_empty.unwrap_or(false),
        )?))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "rename_fields"
    }
}

impl RenameFields {
    pub fn new(fields: IndexMap<Lookup, Lookup>, drop_empty: bool) -> crate::Result<Self> {
        Ok(RenameFields { fields, drop_empty })
    }
}

impl Transform for RenameFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(RenameFieldsEventProcessed);

        for (old_key, new_key) in &self.fields {
            let old_key_string = old_key.to_string(); // TODO: Step 6 of https://github.com/timberio/vector/blob/c4707947bd876a0ff7d7aa36717ae2b32b731593/rfcs/2020-05-25-more-usable-logevents.md#sales-pitch.
            let new_key_string = new_key.to_string(); // TODO: Step 6 of https://github.com/timberio/vector/blob/c4707947bd876a0ff7d7aa36717ae2b32b731593/rfcs/2020-05-25-more-usable-logevents.md#sales-pitch.
            let log = event.as_mut_log();
            match log.remove_prune(&old_key_string, self.drop_empty) {
                Some(v) => {
                    if event.as_mut_log().insert(&new_key_string, v).is_some() {
                        emit!(RenameFieldsFieldOverwritten {
                            field: &old_key_string
                        });
                    }
                }
                None => {
                    emit!(RenameFieldsFieldDoesNotExist {
                        field: &old_key_string
                    });
                }
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn rename_fields() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("to_move", "some value");
        event.as_mut_log().insert("do_not_move", "not moved");
        let mut fields = IndexMap::new();
        fields.insert(
            Lookup::try_from("to_move").unwrap(),
            Lookup::try_from("moved").unwrap(),
        );
        fields.insert(
            Lookup::try_from("not_present").unwrap(),
            Lookup::try_from("should_not_exist").unwrap(),
        );

        let mut transform = RenameFields::new(fields, false).unwrap();

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
