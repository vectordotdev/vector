use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    TransformDescription::new_without_default::<AddTagsConfig>("add_tags")
}

#[typetag::serde(name = "add_tags")]
impl TransformConfig for AddTagsConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
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
        if !self.tags.is_empty() {
            let ref mut tags = event.as_mut_metric().tags;

            if tags.is_none() {
                *tags = Some(BTreeMap::new());
            }

            for (name, value) in &self.tags {
                let map = tags.as_mut().unwrap(); // initialized earlier
                if self.overwrite {
                    map.insert(name.to_string(), value.to_string());
                } else {
                    map.entry(name.to_string()).or_insert(value.to_string());
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
