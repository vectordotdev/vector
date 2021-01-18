use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformDescription},
    event::Event,
    internal_events::{AddTagsTagNotOverwritten, AddTagsTagOverwritten},
    transforms::{FunctionTransform, Transform},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{btree_map::Entry, BTreeMap};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AddTagsConfig {
    pub tags: IndexMap<String, String>,
    #[serde(default = "crate::serde::default_true")]
    pub overwrite: bool,
}

#[derive(Clone, Debug)]
pub struct AddTags {
    tags: IndexMap<String, String>,
    overwrite: bool,
}

inventory::submit! {
    TransformDescription::new::<AddTagsConfig>("add_tags")
}

impl GenerateConfig for AddTagsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            tags: std::iter::once(("name".to_owned(), "value".to_owned())).collect(),
            overwrite: true,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "add_tags")]
impl TransformConfig for AddTagsConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Ok(Transform::function(AddTags::new(
            self.tags.clone(),
            self.overwrite,
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn transform_type(&self) -> &'static str {
        "add_tags"
    }
}

impl AddTags {
    pub fn new(tags: IndexMap<String, String>, overwrite: bool) -> Self {
        AddTags { tags, overwrite }
    }
}

impl FunctionTransform for AddTags {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        if !self.tags.is_empty() {
            let tags = &mut event.as_mut_metric().series.tags;

            if tags.is_none() {
                *tags = Some(BTreeMap::new());
            }

            for (name, value) in &self.tags {
                let map = tags.as_mut().unwrap(); // initialized earlier

                let entry = map.entry(name.to_string());
                match (entry, self.overwrite) {
                    (Entry::Vacant(entry), _) => {
                        entry.insert(value.clone());
                    }
                    (Entry::Occupied(mut entry), true) => {
                        emit!(AddTagsTagOverwritten { tag: name.as_ref() });
                        entry.insert(value.clone());
                    }
                    (Entry::Occupied(_entry), false) => {
                        emit!(AddTagsTagNotOverwritten { tag: name.as_ref() })
                    }
                }
            }
        }

        output.push(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::metric::{Metric, MetricKind, MetricValue},
        event::Event,
    };
    use indexmap::IndexMap;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AddTagsConfig>();
    }

    #[test]
    fn add_tags() {
        let event = Event::Metric(Metric::new(
            "bar".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Gauge { value: 10.0 },
        ));

        let map: IndexMap<String, String> = vec![
            ("region".into(), "us-east-1".into()),
            ("host".into(), "localhost".into()),
        ]
        .into_iter()
        .collect();

        let mut transform = AddTags::new(map, true);
        let metric = transform.transform_one(event).unwrap().into_metric();
        let tags = metric.tags().unwrap();

        assert_eq!(tags.len(), 2);
        assert_eq!(tags.get("region"), Some(&"us-east-1".to_owned()));
        assert_eq!(tags.get("host"), Some(&"localhost".to_owned()));
    }

    #[test]
    fn add_tags_override() {
        let mut tags = BTreeMap::new();
        tags.insert("region".to_string(), "us-east-1".to_string());
        let event = Event::Metric(Metric::new(
            "bar".into(),
            None,
            None,
            Some(tags),
            MetricKind::Absolute,
            MetricValue::Gauge { value: 10.0 },
        ));

        let map: IndexMap<String, String> = vec![("region".to_string(), "overridden".to_string())]
            .into_iter()
            .collect();

        let mut transform = AddTags::new(map, false);

        let metric = transform.transform_one(event).unwrap().into_metric();
        let tags = metric.tags().unwrap();

        assert_eq!(tags.get("region"), Some(&"us-east-1".to_owned()));
    }
}
