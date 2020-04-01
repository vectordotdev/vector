use super::Transform;
use crate::{
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    Event,
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveFieldsConfig {
    fields: Vec<Atom>,
    drop_empty: Option<bool>,
}

pub struct RemoveFields {
    fields: Vec<Atom>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new_without_default::<RemoveFieldsConfig>("remove_fields")
}

#[typetag::serde(name = "remove_fields")]
impl TransformConfig for RemoveFieldsConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(RemoveFields::new(
            self.fields.clone(),
            self.drop_empty.unwrap_or(false),
        )))
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
    pub fn new(fields: Vec<Atom>, drop_empty: bool) -> Self {
        RemoveFields { fields, drop_empty }
    }
}

impl Transform for RemoveFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();
        for field in &self.fields {
            let old_val = log.remove_prune(field, self.drop_empty);
            if old_val.is_none() {
                debug!(
                    message = "Field did not exist",
                    field = field.as_ref(),
                    rate_limit_secs = 30,
                )
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

        let mut transform = RemoveFields::new(vec!["to_remove".into(), "unknown".into()], false);

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get(&"to_remove".into()).is_none());
        assert!(new_event.as_log().get(&"unknown".into()).is_none());
        assert_eq!(
            new_event.as_log()[&"to_keep".into()],
            "another value".into()
        );
    }
}
