use super::Transform;
use crate::{
    topology::config::{DataType, TransformConfig},
    Event,
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveTagsConfig {
    pub tags: Vec<Atom>,
}

pub struct RemoveTags {
    tags: Vec<Atom>,
}

#[typetag::serde(name = "remove_tags")]
impl TransformConfig for RemoveTagsConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(RemoveTags::new(self.tags.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }
}

impl RemoveTags {
    pub fn new(tags: Vec<Atom>) -> Self {
        RemoveTags { tags }
    }
}

impl Transform for RemoveTags {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        for tag in &self.tags {
            let tags = event.as_mut_metric().tags_mut();
            if let Some(tags) = tags {
                tags.remove(tag.as_ref());
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RemoveTags;
    use crate::{event::Event, event::Metric, transforms::Transform};

    #[test]
    fn remove_tags() {
        let event = Event::Metric(Metric::Counter {
            name: "foo".into(),
            val: 10.0,
            timestamp: None,
            tags: Some(
                vec![
                    ("env".to_owned(), "production".to_owned()),
                    ("region".to_owned(), "us-east-1".to_owned()),
                    ("host".to_owned(), "127.0.0.1".to_owned()),
                ]
                .into_iter()
                .collect(),
            ),
        });

        let mut transform = RemoveTags::new(vec!["region".into(), "host".into()]);
        let metric = transform.transform(event).unwrap().into_metric();

        let tags = metric.tags().as_ref().unwrap();
        assert!(tags.contains_key("env"));
        assert!(!tags.contains_key("region"));
        assert!(!tags.contains_key("host"));
    }
}
