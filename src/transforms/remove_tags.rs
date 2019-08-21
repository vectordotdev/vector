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
        let tags = event.as_mut_metric().tags_mut();

        if let Some(map) = tags {
            for tag in &self.tags {
                map.remove(tag.as_ref());

                if map.is_empty() {
                    *tags = None;
                    break;
                }
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

        assert_eq!(tags.len(), 1);
        assert!(tags.contains_key("env"));
        assert!(!tags.contains_key("region"));
        assert!(!tags.contains_key("host"));
    }

    #[test]
    fn remove_all_tags() {
        let event = Event::Metric(Metric::Counter {
            name: "foo".into(),
            val: 10.0,
            timestamp: None,
            tags: Some(
                vec![("env".to_owned(), "production".to_owned())]
                    .into_iter()
                    .collect(),
            ),
        });

        let mut transform = RemoveTags::new(vec!["env".into()]);
        let metric = transform.transform(event).unwrap().into_metric();

        assert!(metric.tags().is_none());
    }

    #[test]
    fn remove_tags_from_none() {
        let event = Event::Metric(Metric::Set {
            name: "foo".into(),
            val: "bar".into(),
            timestamp: None,
            tags: None,
        });

        let mut transform = RemoveTags::new(vec!["env".into()]);
        let metric = transform.transform(event).unwrap().into_metric();

        assert!(metric.tags().is_none());
    }
}
