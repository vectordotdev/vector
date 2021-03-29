use crate::{
    config::{DataType, GenerateConfig, GlobalOptions, TransformConfig, TransformDescription},
    event::Event,
    event::LookupBuf,
    internal_events::{RenameFieldsFieldDoesNotExist, RenameFieldsFieldOverwritten},
    serde::Fields,
    transforms::{FunctionTransform, Transform},
};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RenameFieldsConfig {
    pub fields: Fields<LookupBuf>,
    drop_empty: Option<bool>,
}

#[derive(Debug, Clone)]
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
    async fn build(&self, _globals: &GlobalOptions) -> crate::Result<Transform> {
        let mut fields = IndexMap::default();
        for (key, value) in self.fields.clone().all_fields() {
            fields.insert(
                key.to_string().parse::<LookupBuf>()?,
                value.to_string().parse::<LookupBuf>()?,
            );
        }
        Ok(Transform::function(RenameFields::new(
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

impl FunctionTransform for RenameFields {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        for (old_key, new_key) in &self.fields {
            let log = event.as_mut_log();
            match log.remove(old_key, self.drop_empty) {
                Some(v) => {
                    if event.as_mut_log().insert(new_key.clone(), v).is_some() {
                        emit!(RenameFieldsFieldOverwritten { field: &old_key });
                    }
                }
                None => {
                    emit!(RenameFieldsFieldDoesNotExist { field: &old_key });
                }
            }
        }

        output.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::log_schema,
        event::{Lookup, LookupBuf},
        log_event,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RenameFieldsConfig>();
    }

    #[test]
    fn rename_fields() {
        let mut event = log_event! {
            log_schema().message_key().clone() => "message".to_string(),
            log_schema().timestamp_key().clone() => chrono::Utc::now(),
        };
        event
            .as_mut_log()
            .insert(LookupBuf::from("to_move"), "some value");
        event
            .as_mut_log()
            .insert(LookupBuf::from("do_not_move"), "not moved");
        let mut fields = IndexMap::new();
        fields.insert(LookupBuf::from("to_move"), LookupBuf::from("moved"));
        fields.insert(LookupBuf::from("to_move"), LookupBuf::from("moved"));
        fields.insert(
            LookupBuf::from("not_present"),
            LookupBuf::from("should_not_exist"),
        );

        let mut transform = RenameFields::new(fields, false).unwrap();

        let new_event = transform.transform_one(event).unwrap();

        assert!(new_event.as_log().get(Lookup::from("to_move")).is_none());
        assert_eq!(
            new_event.as_log()[Lookup::from("moved")],
            "some value".into()
        );
        assert!(new_event
            .as_log()
            .get(Lookup::from("not_present"))
            .is_none());
        assert!(new_event
            .as_log()
            .get(Lookup::from("should_not_exist"))
            .is_none());
        assert_eq!(
            new_event.as_log()[Lookup::from("do_not_move")],
            "not moved".into()
        );
    }
}
