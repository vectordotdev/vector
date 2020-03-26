use super::Transform;
use crate::{
    event::Event,
    serde::Fields,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RenameFieldsConfig {
    pub fields: Fields<String>,
    drop_empty: Option<bool>,
}

pub struct RenameFields {
    fields: IndexMap<Atom, Atom>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new_without_default::<RenameFieldsConfig>("rename_fields")
}

#[typetag::serde(name = "rename_fields")]
impl TransformConfig for RenameFieldsConfig {
    fn build(&self, _exec: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(RenameFields::new(
            self.fields
                .clone()
                .all_fields()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
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
    pub fn new(fields: IndexMap<Atom, Atom>, drop_empty: bool) -> crate::Result<Self> {
        Ok(RenameFields { fields, drop_empty })
    }
}

impl Transform for RenameFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        for (old_key, new_key) in &self.fields {
            let log = event.as_mut_log();
            match log.remove_prune(&old_key, self.drop_empty) {
                Some(v) => {
                    if let Some(_) = event.as_mut_log().insert(&new_key.clone(), v) {
                        debug!(
                            message = "Field overwritten",
                            field = old_key.as_ref(),
                            rate_limit_secs = 30,
                        )
                    }
                }
                None => {
                    debug!(
                        message = "Field did not exist",
                        field = old_key.as_ref(),
                        rate_limit_secs = 30,
                    );
                }
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
