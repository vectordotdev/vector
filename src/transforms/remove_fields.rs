use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    internal_events::{RemoveFieldsEventProcessed, RemoveFieldsFieldMissing},
    Event,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveFieldsConfig {
    fields: Vec<String>,
    drop_empty: Option<bool>,
}

pub struct RemoveFields {
    fields: Vec<String>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new::<RemoveFieldsConfig>("remove_fields")
}

impl GenerateConfig for RemoveFieldsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            fields: Vec::new(),
            drop_empty: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "remove_fields")]
impl TransformConfig for RemoveFieldsConfig {
    async fn build(&self) -> crate::Result<Box<dyn Transform>> {
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
    pub fn new(fields: Vec<String>, drop_empty: bool) -> crate::Result<Self> {
        Ok(RemoveFields { fields, drop_empty })
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
    use super::{RemoveFields, RemoveFieldsConfig};
    use crate::{event::Event, transforms::Transform};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemoveFieldsConfig>();
    }

    #[test]
    fn remove_fields() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("to_remove", "some value");
        event.as_mut_log().insert("to_keep", "another value");

        let mut transform =
            RemoveFields::new(vec!["to_remove".into(), "unknown".into()], false).unwrap();

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get("to_remove").is_none());
        assert!(new_event.as_log().get("unknown").is_none());
        assert_eq!(new_event.as_log()["to_keep"], "another value".into());
    }
}
