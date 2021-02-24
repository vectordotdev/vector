use crate::{
    config::{DataType, GenerateConfig, GlobalOptions, TransformConfig, TransformDescription},
    event::{Event, Value},
    internal_events::{ExplodeFieldIsNotArray, ExplodeFieldMissing, ExplodeFieldOverwritten},
    transforms::{FunctionTransform, Transform},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ExplodeConfig {
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub rename_as: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Explode {
    field: String,
    rename_as: Option<String>,
}

inventory::submit! {
    TransformDescription::new::<ExplodeConfig>("explode")
}

impl GenerateConfig for ExplodeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            field: None,
            rename_as: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "explode")]
impl TransformConfig for ExplodeConfig {
    async fn build(&self, _globals: &GlobalOptions) -> crate::Result<Transform> {
        let field = self
            .field
            .clone()
            .unwrap_or_else(|| crate::config::log_schema().message_key().to_string());

        Ok(Transform::function(Explode::new(
            field,
            self.rename_as.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "explode"
    }
}

impl Explode {
    pub fn new(field: String, rename_as: Option<String>) -> Self {
        Explode { field, rename_as }
    }
}

impl FunctionTransform for Explode {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        let target_field = if let Some(rename_as) = &self.rename_as {
            rename_as
        } else {
            &self.field
        };
        if let Some(value) = event.as_mut_log().remove_prune(&self.field, true) {
            if let Value::Array(array) = value {
                if event.as_mut_log().insert(&target_field, Value::Null).is_some() {
                    emit!(ExplodeFieldOverwritten {
                        field: &target_field
                    });
                }
                for v in array.into_iter() {
                    let mut new_event = event.clone();
                    new_event.as_mut_log().insert(&target_field, v);
                    output.push(new_event);
                }
            } else {
                emit!(ExplodeFieldIsNotArray {
                    field: &self.field,
                    kind: value.kind()
                });
            }
        } else {
            emit!(ExplodeFieldMissing { field: &self.field });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use indexmap::IndexMap;
    use std::iter::FromIterator;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ExplodeConfig>();
    }

    #[test]
    fn explode() {
        let mut event = Event::from("");
        event
            .as_mut_log()
            .insert("array_key", vec!["array_val1", "array_val2"]);

        let mut transform = Explode::new("array_key".to_string(), None);

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 2);
        assert_eq!(
            new_events[0].as_log().get_flat("array_key").unwrap(),
            &Value::from("array_val1".to_string())
        );
        assert_eq!(
            new_events[1].as_log().get_flat("array_key").unwrap(),
            &Value::from("array_val2".to_string())
        );
    }

    #[test]
    fn explode_rename() {
        let mut event = Event::from("");
        event
            .as_mut_log()
            .insert("array_key", vec!["array_val1", "array_val2"]);

        let mut transform = Explode::new("array_key".to_string(), Some("renamed_key".to_string()));

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 2);
        assert!(new_events[0].as_log().get_flat("array_key").is_none());
        assert_eq!(
            new_events[0].as_log().get_flat("renamed_key").unwrap(),
            &Value::from("array_val1".to_string())
        );
        assert!(new_events[1].as_log().get_flat("array_key").is_none());
        assert_eq!(
            new_events[1].as_log().get_flat("renamed_key").unwrap(),
            &Value::from("array_val2".to_string())
        );
    }

    #[test]
    fn explode_rename_as_original_field() {
        let mut event = Event::from("");
        event
            .as_mut_log()
            .insert("array_key", vec!["array_val1", "array_val2"]);

        let mut transform = Explode::new("array_key".to_string(), Some("array_key".to_string()));

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 2);
        assert_eq!(
            new_events[0].as_log().get_flat("array_key").unwrap(),
            &Value::from("array_val1".to_string())
        );
        assert_eq!(
            new_events[1].as_log().get_flat("array_key").unwrap(),
            &Value::from("array_val2".to_string())
        );
    }

    #[test]
    fn explode_rename_to_overwride_other_field() {
        let mut event = Event::from("");
        event
            .as_mut_log()
            .insert("array_key", vec!["array_val1", "array_val2"]);
        event
            .as_mut_log()
            .insert("overwritten_key", "overwritten_value");

        let mut transform =
            Explode::new("array_key".to_string(), Some("overwritten_key".to_string()));

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 2);
        assert_eq!(
            new_events[0].as_log().get_flat("overwritten_key").unwrap(),
            &Value::from("array_val1".to_string())
        );
        assert_eq!(
            new_events[1].as_log().get_flat("overwritten_key").unwrap(),
            &Value::from("array_val2".to_string())
        );
    }

    #[test]
    fn explode_preserves_types() {
        let mut event = Event::from("");
        event.as_mut_log().insert("float", vec![1.0, 2.0]);
        event.as_mut_log().insert("int", vec![1, 2]);
        event.as_mut_log().insert("string", vec!["a", "b"]);
        event.as_mut_log().insert("bool", vec![true, false]);
        event
            .as_mut_log()
            .insert("array", vec![vec![1, 2], vec![3, 4]]);
        let mut map1 = IndexMap::new();
        map1.insert(String::from("key"), Value::from("value"));
        let mut map2 = IndexMap::new();
        map2.insert(String::from("key2"), Value::from("value2"));
        event.as_mut_log().insert(
            String::from("table"),
            vec![Value::from_iter(map1), Value::from_iter(map2)],
        );

        for kind in vec!["float", "int", "string", "bool", "array", "table"].into_iter() {
            let mut transform = Explode::new(kind.to_string(), None);
            let mut new_events = Vec::new();
            transform.transform(&mut new_events, event.clone());
            assert_eq!(new_events.len(), 2);
            let event = new_events[0].clone().into_log();
            match kind {
                "float" => assert_eq!(event["float"], 1.0.into()),
                "int" => assert_eq!(event["int"], 1.into()),
                "string" => assert_eq!(event["string"], "a".into()),
                "bool" => assert_eq!(event["bool"], true.into()),
                "array" => {
                    assert_eq!(event["array[0]"], 1.into());
                    assert_eq!(event["array[1]"], 2.into());
                }
                "table" => assert_eq!(event["table.key"], "value".into()),
                _ => unreachable!("unknown find"),
            }
        }
    }

    #[test]
    fn explode_non_exists_field() {
        let mut event = Event::from("");
        event
            .as_mut_log()
            .insert("array_key", vec!["array_val1", "array_val2"]);

        let mut transform =
            Explode::new("non_exists_key".to_string(), Some("other_key".to_string()));

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 0);
    }

    #[test]
    fn explode_non_array_field() {
        let mut event = Event::from("");
        event.as_mut_log().insert("non_array_key", "string_value");

        let mut transform =
            Explode::new("non_array_key".to_string(), Some("other_key".to_string()));

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 0);
    }

    #[test]
    fn explode_nested_field() {
        let mut event = Event::from("");
        let mut map = IndexMap::new();
        map.insert(
            String::from("child_key"),
            Value::from(vec!["array_val1", "array_val2"]),
        );
        event
            .as_mut_log()
            .insert(String::from("parent_key"), Value::from_iter(map));

        let mut transform = Explode::new(
            "parent_key.child_key".to_string(),
            Some("renamed_parent_key.renamed_child_key".to_string()),
        );

        let mut new_events = Vec::new();
        transform.transform(&mut new_events, event);

        assert_eq!(new_events.len(), 2);
        assert!(new_events[0].as_log().get("parent_key.child_key").is_none());
        assert!(new_events[0].as_log().get_flat("parent_key").is_none());
        assert_eq!(
            new_events[0]
                .as_log()
                .get("renamed_parent_key.renamed_child_key")
                .unwrap(),
            &Value::from("array_val1".to_string())
        );
        assert!(new_events[1].as_log().get("parent_key.child_key").is_none());
        assert!(new_events[1].as_log().get_flat("parent_key").is_none());
        assert_eq!(
            new_events[1]
                .as_log()
                .get("renamed_parent_key.renamed_child_key")
                .unwrap(),
            &Value::from("array_val2".to_string())
        );
    }
}
