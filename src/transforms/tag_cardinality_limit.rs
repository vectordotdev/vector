use super::Transform;
use crate::{
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    Event,
};
use bloom::{BloomFilter, ASMS};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed
//#[serde(deny_unknown_fields)]
pub struct TagCardinalityLimitConfig {
    #[serde(default = "default_value_limit")]
    pub value_limit: u32,

    #[serde(default = "default_limit_exceeded_action")]
    pub limit_exceeded_action: LimitExceededAction,

    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mode {
    Exact,
    Probabilistic(BloomFilterConfig),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BloomFilterConfig {
    #[serde(default = "default_cache_size")]
    pub cache_size_per_key: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "limit_exceeded_action", rename_all = "snake_case")]
pub enum LimitExceededAction {
    DropTag,
    DropEvent,
}

pub struct TagCardinalityLimit {
    config: TagCardinalityLimitConfig,
    accepted_tags: HashMap<String, TagValueSet>,
}

fn default_limit_exceeded_action() -> LimitExceededAction {
    LimitExceededAction::DropTag
}

fn default_value_limit() -> u32 {
    500
}

fn default_cache_size() -> usize {
    5000 * 1024 // 5KB
}

inventory::submit! {
    TransformDescription::new_without_default::<TagCardinalityLimitConfig>("tag_cardinality_limit")
}

#[typetag::serde(name = "tag_cardinality_limit")]
impl TransformConfig for TagCardinalityLimitConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(TagCardinalityLimit::new(self.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn transform_type(&self) -> &'static str {
        "tag_cardinality_limit"
    }
}

/// Container for storing the set of accepted values for a given tag key.
struct TagValueSet {
    storage: TagValueSetStorage,
    num_elements: usize,
}

enum TagValueSetStorage {
    Set(HashSet<String>),
    Bloom(BloomFilter),
}

impl TagValueSet {
    fn new(value_limit: u32, mode: &Mode) -> Self {
        match &mode {
            Mode::Exact => Self {
                storage: TagValueSetStorage::Set(HashSet::with_capacity(value_limit as usize)),
                num_elements: 0,
            },
            Mode::Probabilistic(config) => {
                let num_bits = config.cache_size_per_key / 8; // Convert bytes to bits
                let num_hashes = bloom::optimal_num_hashes(num_bits, value_limit);

                Self {
                    storage: TagValueSetStorage::Bloom(BloomFilter::with_size(
                        num_bits, num_hashes,
                    )),
                    num_elements: 0,
                }
            }
        }
    }

    fn contains(&self, val: &String) -> bool {
        match &self.storage {
            TagValueSetStorage::Set(set) => set.contains(val),
            TagValueSetStorage::Bloom(bloom) => bloom.contains(val),
        }
    }

    fn len(&self) -> usize {
        self.num_elements
    }

    fn insert(&mut self, val: &String) -> bool {
        let inserted = match &mut self.storage {
            TagValueSetStorage::Set(set) => set.insert(val.clone()),
            TagValueSetStorage::Bloom(bloom) => bloom.insert(val),
        };
        if inserted {
            self.num_elements += 1
        }
        inserted
    }
}

impl TagCardinalityLimit {
    fn new(config: TagCardinalityLimitConfig) -> TagCardinalityLimit {
        TagCardinalityLimit {
            config,
            accepted_tags: HashMap::new(),
        }
    }

    /// Takes in key and a value corresponding to a tag on an incoming Metric Event.
    /// If that value is already part of set of accepted values for that key, then simply returns
    /// true.  If that value is not yet part of the accepted values for that key, checks whether
    /// we have hit the value_limit for that key yet and if not adds the value to the set of
    /// accepted values for the key and returns true, otherwise returns false.  A false return
    /// value indicates to the caller that the value is not accepted for this key, and the
    /// configured limit_exceeded_action should be taken.
    fn try_accept_tag(&mut self, key: &String, value: &String) -> bool {
        if !self.accepted_tags.contains_key(key) {
            self.accepted_tags.insert(
                key.clone(),
                TagValueSet::new(self.config.value_limit, &self.config.mode),
            );
        }
        let tag_value_set = self.accepted_tags.get_mut(key).unwrap();

        if tag_value_set.contains(value) {
            // Tag value has already been accepted, nothing more to do.
            return true;
        }

        // Tag value not yet part of the accepted set.
        if tag_value_set.len() < self.config.value_limit as usize {
            // accept the new value
            tag_value_set.insert(value);

            if tag_value_set.len() == self.config.value_limit as usize {
                warn!(
                    "value_limit reached for key {}. New values for this key will be rejected",
                    key
                );
            }

            true
        } else {
            // New tag value is rejected.
            false
        }
    }
}

impl Transform for TagCardinalityLimit {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        match event.as_mut_metric().tags {
            Some(ref mut tags_map) => {
                match self.config.limit_exceeded_action {
                    LimitExceededAction::DropEvent => {
                        for (key, value) in tags_map {
                            if !self.try_accept_tag(key, value) {
                                info!(
                                    message = "Rejecting Metric Event containing tag with new value after hitting configured 'value_limit'",
                                    tag_key = key.as_str(),
                                    tag_value = &value.as_str(),
                                    rate_limit_secs = 10,
                                );
                                return None;
                            }
                        }
                    }
                    LimitExceededAction::DropTag => {
                        let mut to_delete = Vec::new();
                        for (key, value) in tags_map.iter() {
                            if !self.try_accept_tag(key, value) {
                                info!(
                                    message =
                                        "Rejecting tag after hitting configured 'value_limit'",
                                    tag_key = key.as_str(),
                                    tag_value = value.as_str(),
                                    rate_limit_secs = 10,
                                );
                                to_delete.push(key.clone());
                            }
                        }
                        for key in to_delete {
                            tags_map.remove(&key);
                        }
                    }
                }
                Some(event)
            }
            None => Some(event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LimitExceededAction, TagCardinalityLimit, TagCardinalityLimitConfig};
    use crate::transforms::tag_cardinality_limit::{default_cache_size, BloomFilterConfig, Mode};
    use crate::{event::metric, event::Event, event::Metric, transforms::Transform};
    use std::collections::BTreeMap;

    fn make_metric(tags: BTreeMap<String, String>) -> Event {
        Event::Metric(Metric {
            name: "event".into(),
            timestamp: None,
            tags: Some(tags),
            kind: metric::MetricKind::Incremental,
            value: metric::MetricValue::Counter { value: 1.0 },
        })
    }

    fn make_transform_hashset(
        value_limit: u32,
        limit_exceeded_action: LimitExceededAction,
    ) -> TagCardinalityLimit {
        TagCardinalityLimit::new(TagCardinalityLimitConfig {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Exact,
        })
    }

    fn make_transform_bloom(
        value_limit: u32,
        limit_exceeded_action: LimitExceededAction,
    ) -> TagCardinalityLimit {
        TagCardinalityLimit::new(TagCardinalityLimitConfig {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
        })
    }

    #[test]
    fn tag_cardinality_limit_drop_event_hashset() {
        drop_event(make_transform_hashset(2, LimitExceededAction::DropEvent));
    }

    #[test]
    fn tag_cardinality_limit_drop_event_bloom() {
        drop_event(make_transform_bloom(2, LimitExceededAction::DropEvent));
    }

    fn drop_event(mut transform: TagCardinalityLimit) {
        let tags1: BTreeMap<String, String> =
            vec![("tag1".into(), "val1".into())].into_iter().collect();
        let event1 = make_metric(tags1);

        let tags2: BTreeMap<String, String> =
            vec![("tag1".into(), "val2".into())].into_iter().collect();
        let event2 = make_metric(tags2);

        let tags3: BTreeMap<String, String> =
            vec![("tag1".into(), "val3".into())].into_iter().collect();
        let event3 = make_metric(tags3);

        let new_event1 = transform.transform(event1.clone()).unwrap();
        let new_event2 = transform.transform(event2.clone()).unwrap();
        let new_event3 = transform.transform(event3.clone());

        assert_eq!(new_event1, event1);
        assert_eq!(new_event2, event2);
        // Third value rejected since value_limit is 2.
        assert_eq!(None, new_event3);
    }

    #[test]
    fn tag_cardinality_limit_drop_tag_hashset() {
        drop_tag(make_transform_hashset(2, LimitExceededAction::DropTag));
    }

    #[test]
    fn tag_cardinality_limit_drop_tag_bloom() {
        drop_tag(make_transform_bloom(2, LimitExceededAction::DropTag));
    }

    fn drop_tag(mut transform: TagCardinalityLimit) {
        let tags1: BTreeMap<String, String> = vec![
            ("tag1".into(), "val1".into()),
            ("tag2".into(), "val1".into()),
        ]
        .into_iter()
        .collect();
        let event1 = make_metric(tags1);

        let tags2: BTreeMap<String, String> = vec![
            ("tag1".into(), "val2".into()),
            ("tag2".into(), "val1".into()),
        ]
        .into_iter()
        .collect();
        let event2 = make_metric(tags2);

        let tags3: BTreeMap<String, String> = vec![
            ("tag1".into(), "val3".into()),
            ("tag2".into(), "val1".into()),
        ]
        .into_iter()
        .collect();
        let event3 = make_metric(tags3);

        let new_event1 = transform.transform(event1.clone()).unwrap();
        let new_event2 = transform.transform(event2.clone()).unwrap();
        let new_event3 = transform.transform(event3.clone()).unwrap();

        assert_eq!(new_event1, event1);
        assert_eq!(new_event2, event2);
        // The third event should have been modified to remove "tag1"
        assert_ne!(new_event3, event3);
        assert!(!new_event3
            .as_metric()
            .tags
            .as_ref()
            .unwrap()
            .contains_key("tag1"));
        assert_eq!(
            "val1",
            new_event3
                .as_metric()
                .tags
                .as_ref()
                .unwrap()
                .get("tag2")
                .unwrap()
        );
    }

    #[test]
    fn tag_cardinality_limit_separate_value_limit_per_tag_hashset() {
        separate_value_limit_per_tag(make_transform_hashset(2, LimitExceededAction::DropEvent));
    }

    #[test]
    fn tag_cardinality_limit_separate_value_limit_per_tag_bloom() {
        separate_value_limit_per_tag(make_transform_bloom(2, LimitExceededAction::DropEvent));
    }

    /// Test that hitting the value limit on one tag does not affect the ability to take new
    /// values for other tags.
    fn separate_value_limit_per_tag(mut transform: TagCardinalityLimit) {
        let tags1: BTreeMap<String, String> = vec![
            ("tag1".into(), "val1".into()),
            ("tag2".into(), "val1".into()),
        ]
        .into_iter()
        .collect();
        let event1 = make_metric(tags1);

        let tags2: BTreeMap<String, String> = vec![
            ("tag1".into(), "val2".into()),
            ("tag2".into(), "val1".into()),
        ]
        .into_iter()
        .collect();
        let event2 = make_metric(tags2);

        // Now value limit is reached for "tag1", but "tag2" still has values available.
        let tags3: BTreeMap<String, String> = vec![
            ("tag1".into(), "val1".into()),
            ("tag1".into(), "val2".into()),
        ]
        .into_iter()
        .collect();
        let event3 = make_metric(tags3);

        let new_event1 = transform.transform(event1.clone()).unwrap();
        let new_event2 = transform.transform(event2.clone()).unwrap();
        let new_event3 = transform.transform(event3.clone()).unwrap();

        assert_eq!(new_event1, event1);
        assert_eq!(new_event2, event2);
        assert_eq!(new_event3, event3);
    }
}
