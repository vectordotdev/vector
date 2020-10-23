use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    event::LookupBuf,
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
    pub fields: Fields<LookupBuf>,
    drop_empty: Option<bool>,
}

pub struct RenameFields {
    fields: IndexMap<LookupBuf, LookupBuf>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new::<RenameFieldsConfig>("rename_fields")
}

impl GenerateConfig for RenameFieldsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(r#"fields.old_field_name = "new_field_name""#).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "rename_fields")]
impl TransformConfig for RenameFieldsConfig {
    async fn build(&self, _exec: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let mut fields = IndexMap::default();
        for (key, value) in self.fields.clone().all_fields() {
            fields.insert(
                key.to_string().parse::<LookupBuf>()?,
                value.to_string().parse::<LookupBuf>()?,
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
    pub fn new(fields: IndexMap<LookupBuf, LookupBuf>, drop_empty: bool) -> crate::Result<Self> {
        Ok(RenameFields { fields, drop_empty })
    }
}

impl Transform for RenameFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(RenameFieldsEventProcessed);

        for (old_key, new_key) in &self.fields {
            let log = event.as_mut_log();
            match log.remove(&old_key, self.drop_empty) {
                Some(v) => {
                    if event.as_mut_log().insert(new_key.clone(), v).is_some() {
                        emit!(RenameFieldsFieldOverwritten { field: old_key.as_lookup() });
                    }
                }
                None => {
                    emit!(RenameFieldsFieldDoesNotExist { field: old_key.as_lookup() });
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
    fn generate_config() {
        crate::test_util::test_generate_config::<RenameFieldsConfig>();
    }

    #[test]
    fn rename_fields() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("to_move", "some value");
        event.as_mut_log().insert("do_not_move", "not moved");
        let mut fields = IndexMap::new();
        fields.insert(
            LookupBuf::try_from("to_move").unwrap(),
            LookupBuf::try_from("moved").unwrap(),
        );
        fields.insert(
            LookupBuf::try_from("not_present").unwrap(),
            LookupBuf::try_from("should_not_exist").unwrap(),
        );

        let mut transform = RenameFields::new(fields, false).unwrap();

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get("to_move").is_none());
        assert_eq!(new_event.as_log()["moved"], "some value".into());
        assert!(new_event.as_log().get("not_present").is_none());
        assert!(new_event.as_log().get("should_not_exist").is_none());
        assert_eq!(new_event.as_log()["do_not_move"], "not moved".into());
    }
}
