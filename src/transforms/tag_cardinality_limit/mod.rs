use futures::{Stream, StreamExt};
use hashbrown::HashMap;
use std::{future::ready, pin::Pin};

use crate::transforms::tag_cardinality_limit::config::LimitExceededAction;
use crate::{
    event::Event,
    internal_events::{
        TagCardinalityLimitRejectingEvent, TagCardinalityLimitRejectingTag,
        TagCardinalityValueLimitReached,
    },
    transforms::TaskTransform,
};

mod config;
mod tag_value_set;

#[cfg(test)]
mod tests;

use crate::event::metric::TagValueSet;
pub use config::TagCardinalityLimitConfig;
use tag_value_set::AcceptedTagValueSet;

#[derive(Debug)]
pub struct TagCardinalityLimit {
    config: TagCardinalityLimitConfig,
    accepted_tags: HashMap<String, AcceptedTagValueSet>,
}

impl TagCardinalityLimit {
    fn new(config: TagCardinalityLimitConfig) -> Self {
        Self {
            config,
            accepted_tags: HashMap::new(),
        }
    }

    /// Takes in key and a value corresponding to a tag on an incoming Metric
    /// Event.  If that value is already part of set of accepted values for that
    /// key, then simply returns true.  If that value is not yet part of the
    /// accepted values for that key, checks whether we have hit the value_limit
    /// for that key yet and if not adds the value to the set of accepted values
    /// for the key and returns true, otherwise returns false.  A false return
    /// value indicates to the caller that the value is not accepted for this
    /// key, and the configured limit_exceeded_action should be taken.
    fn try_accept_tag(&mut self, key: &str, value: &TagValueSet) -> bool {
        let tag_value_set = self.accepted_tags.entry_ref(key).or_insert_with(|| {
            AcceptedTagValueSet::new(self.config.value_limit, &self.config.mode)
        });

        if tag_value_set.contains(value) {
            // Tag value has already been accepted, nothing more to do.
            return true;
        }

        // Tag value not yet part of the accepted set.
        if tag_value_set.len() < self.config.value_limit {
            // accept the new value
            tag_value_set.insert(value.clone());

            if tag_value_set.len() == self.config.value_limit {
                emit!(TagCardinalityValueLimitReached { key });
            }

            true
        } else {
            // New tag value is rejected.
            false
        }
    }

    /// Checks if recording a key and value corresponding to a tag on an incoming Metric would
    /// exceed the cardinality limit.
    fn tag_limit_exceeded(&self, key: &str, value: &TagValueSet) -> bool {
        self.accepted_tags
            .get(key)
            .map(|value_set| {
                !value_set.contains(value) && value_set.len() >= self.config.value_limit
            })
            .unwrap_or(false)
    }

    /// Record a key and value corresponding to a tag on an incoming Metric.
    fn record_tag_value(&mut self, key: &str, value: &TagValueSet) {
        self.accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(self.config.value_limit, &self.config.mode))
            .insert(value.clone());
    }

    fn transform_one(&mut self, mut event: Event) -> Option<Event> {
        let metric = event.as_mut_metric();
        let metric_name = metric.name().to_string();
        if let Some(tags_map) = metric.tags_mut() {
            match self.config.limit_exceeded_action {
                LimitExceededAction::DropEvent => {
                    // This needs to check all the tags, to ensure that the ordering of tag names
                    // doesn't change the behavior of the check.

                    for (key, value) in tags_map.iter_sets() {
                        if self.tag_limit_exceeded(key, value) {
                            emit!(TagCardinalityLimitRejectingEvent {
                                metric_name: &metric_name,
                                tag_key: key,
                                tag_value: &value.to_string(),
                            });
                            return None;
                        }
                    }
                    for (key, value) in tags_map.iter_sets() {
                        self.record_tag_value(key, value);
                    }
                }
                LimitExceededAction::DropTag => {
                    tags_map.retain(|key, value| {
                        if self.try_accept_tag(key, value) {
                            true
                        } else {
                            emit!(TagCardinalityLimitRejectingTag {
                                metric_name: &metric_name,
                                tag_key: key,
                                tag_value: &value.to_string(),
                            });
                            false
                        }
                    });
                }
            }
        }
        Some(event)
    }
}

impl TaskTransform<Event> for TagCardinalityLimit {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = self;
        Box::pin(task.filter_map(move |v| ready(inner.transform_one(v))))
    }
}
