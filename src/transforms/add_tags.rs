use super::Transform;
use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddTagsConfig {
    pub tags: IndexMap<Atom, String>,
}

pub struct AddTags {
    tags: IndexMap<Atom, String>,
}

#[typetag::serde(name = "add_tags")]
impl TransformConfig for AddTagsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, crate::Error> {
        Ok(Box::new(AddTags::new(self.tags.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }
}

impl AddTags {
    pub fn new(tags: IndexMap<Atom, String>) -> Self {
        AddTags { tags }
    }
}

impl Transform for AddTags {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        if !self.tags.is_empty() {
            let tags = event.as_mut_metric().tags_mut();

            if tags.is_none() {
                *tags = Some(HashMap::new());
            }

            for (name, value) in &self.tags {
                let map = tags.as_mut().unwrap(); // initialized earlier
                map.insert(name.to_string(), value.to_string());
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::AddTags;
    use crate::{event::Event, event::Metric, transforms::Transform};
    use indexmap::IndexMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn add_tags() {
        let event = Event::Metric(Metric::Gauge {
            name: "bar".into(),
            val: 10.0,
            direction: None,
            timestamp: None,
            tags: None,
        });

        let map: IndexMap<Atom, String> = vec![
            (Atom::from("region"), "us-east-1".into()),
            (Atom::from("host"), "localhost".into()),
        ]
        .into_iter()
        .collect();

        let mut transform = AddTags::new(map);
        let metric = transform.transform(event).unwrap().into_metric();
        let tags = metric.tags().as_ref().unwrap();

        assert_eq!(tags.len(), 2);
        assert_eq!(tags.get("region"), Some(&"us-east-1".to_owned()));
        assert_eq!(tags.get("host"), Some(&"localhost".to_owned()));
    }
}
