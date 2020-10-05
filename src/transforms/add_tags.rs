use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    internal_events::{AddTagsEventProcessed, AddTagsTagNotOverwritten, AddTagsTagOverwritten},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{btree_map::Entry, BTreeMap};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddTagsConfig {
    pub tags: IndexMap<Atom, String>,
    #[serde(default = "crate::serde::default_true")]
    pub overwrite: bool,
}

pub struct AddTags {
    tags: IndexMap<Atom, String>,
    overwrite: bool,
}

inventory::submit! {
    TransformDescription::new::<AddTagsConfig>("add_tags")
}

impl GenerateConfig for AddTagsConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "add_tags")]
impl TransformConfig for AddTagsConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(AddTags::new(self.tags.clone(), self.overwrite)))
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
    pub fn new(tags: IndexMap<Atom, String>, overwrite: bool) -> Self {
        AddTags { tags, overwrite }
    }
}

impl Transform for AddTags {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(AddTagsEventProcessed);

        if !self.tags.is_empty() {
            let tags = &mut event.as_mut_metric().tags;

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

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::AddTags;
    use crate::{
        event::metric::{Metric, MetricKind, MetricValue},
        event::Event,
        transforms::Transform,
    };
    use indexmap::IndexMap;
    use std::collections::BTreeMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn add_tags() {
        let event = Event::Metric(Metric {
            name: "bar".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 10.0 },
        });

        let map: IndexMap<Atom, String> = vec![
            (Atom::from("region"), "us-east-1".into()),
            (Atom::from("host"), "localhost".into()),
        ]
        .into_iter()
        .collect();

        let mut transform = AddTags::new(map, true);
        let metric = transform.transform(event).unwrap().into_metric();
        let tags = metric.tags.unwrap();

        assert_eq!(tags.len(), 2);
        assert_eq!(tags.get("region"), Some(&"us-east-1".to_owned()));
        assert_eq!(tags.get("host"), Some(&"localhost".to_owned()));
    }

    #[test]
    fn add_tags_override() {
        let mut tags = BTreeMap::new();
        tags.insert("region".to_string(), "us-east-1".to_string());
        let event = Event::Metric(Metric {
            name: "bar".into(),
            timestamp: None,
            tags: Some(tags),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 10.0 },
        });

        let map: IndexMap<Atom, String> = vec![(Atom::from("region"), "overridden".into())]
            .into_iter()
            .collect();

        let mut transform = AddTags::new(map, false);

        let metric = transform.transform(event).unwrap().into_metric();
        let tags = metric.tags.unwrap();

        assert_eq!(tags.get("region"), Some(&"us-east-1".to_owned()));
    }
}
