use std::{future::ready, pin::Pin};

use futures::{Stream, StreamExt};
use hashbrown::HashMap;
use vector_lib::{event::Event, transform::TaskTransform};

use crate::internal_events::{
    TagCardinalityLimitRejectingEvent, TagCardinalityLimitRejectingTag,
    TagCardinalityLimitUntracked, TagCardinalityTrackedKeys, TagCardinalityValueLimitReached,
};

pub mod config;
mod tag_value_set;

#[cfg(test)]
mod tests;

pub use config::{
    BloomFilterConfig, Config, Inner, LimitExceededAction, Mode, PerMetricConfig, TrackingScope,
};
use tag_value_set::AcceptedTagValueSet;

use crate::event::metric::TagValueSet;

type MetricId = (Option<String>, String);

/// Outcome of attempting to accept a tag value.
#[derive(Debug, Eq, PartialEq)]
enum AcceptResult {
    /// Tag value is tracked and accepted (under the configured `value_limit`).
    Accepted,
    /// Tag value is tracked but exceeds `value_limit`; caller should drop the tag.
    Rejected,
    /// `max_tracked_keys` was reached and the (metric, tag-key) pair could not be
    /// allocated; caller should pass the tag through unchecked.
    Untracked,
}

#[derive(Debug)]
pub struct TagCardinalityLimit {
    config: Config,
    accepted_tags: HashMap<Option<MetricId>, HashMap<String, AcceptedTagValueSet>>,
    /// Total count of currently-tracked (metric_bucket, tag_key) pairs.
    /// Used to enforce `config.max_tracked_keys`.
    tracked_keys_count: usize,
}

impl TagCardinalityLimit {
    fn new(config: Config) -> Self {
        Self {
            config,
            accepted_tags: HashMap::new(),
            tracked_keys_count: 0,
        }
    }

    /// Returns true if a new tag-key bucket can be allocated without exceeding
    /// `config.max_tracked_keys`. Always returns true when `max_tracked_keys` is unset.
    const fn can_allocate_new_key(&self) -> bool {
        match self.config.max_tracked_keys {
            Some(max) => self.tracked_keys_count < max,
            None => true,
        }
    }

    /// Bumps the tracked-keys counter and emits the gauge sample. Call after a
    /// successful new-key allocation in `accepted_tags`.
    fn record_new_key_allocation(&mut self) {
        self.tracked_keys_count += 1;
        emit!(TagCardinalityTrackedKeys {
            count: self.tracked_keys_count,
        });
    }

    fn get_config_for_metric(&self, metric_key: Option<&MetricId>) -> &Inner {
        match metric_key {
            Some(id) => self
                .config
                .per_metric_limits
                .iter()
                .find(|(name, config)| {
                    **name == id.1 && (config.namespace.is_none() || config.namespace == id.0)
                })
                .map(|(_, c)| &c.config)
                .unwrap_or(&self.config.global),
            None => &self.config.global,
        }
    }

    /// Attempts to accept a tag value for a (metric, tag-key) pair.
    ///
    /// Returns:
    /// - `Accepted` if the value is already tracked, or fits under the configured
    ///   `value_limit` and is now recorded.
    /// - `Rejected` if the value would exceed `value_limit`; the caller should drop
    ///   the tag.
    /// - `Untracked` if a new (metric, tag-key) pair would have to be allocated but
    ///   `max_tracked_keys` has been reached; the caller should pass the tag through
    ///   unchecked and emit the warning metric.
    fn try_accept_tag(
        &mut self,
        metric_key: Option<&MetricId>,
        key: &str,
        value: &TagValueSet,
    ) -> AcceptResult {
        let config = *self.get_config_for_metric(metric_key);
        let metric_key_owned = metric_key.cloned();

        // Determine whether this (metric, tag-key) pair already has a bucket.
        let pair_exists = self
            .accepted_tags
            .get(&metric_key_owned)
            .is_some_and(|m| m.contains_key(key));

        if !pair_exists {
            if !self.can_allocate_new_key() {
                return AcceptResult::Untracked;
            }
            self.record_new_key_allocation();
        }

        let metric_accepted_tags = self.accepted_tags.entry(metric_key_owned).or_default();
        let tag_value_set = metric_accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(config.value_limit, &config.mode));

        if tag_value_set.contains(value) {
            // Tag value has already been accepted, nothing more to do.
            return AcceptResult::Accepted;
        }

        // Tag value not yet part of the accepted set.
        if tag_value_set.len() < config.value_limit {
            // accept the new value
            tag_value_set.insert(value.clone());

            if tag_value_set.len() == config.value_limit {
                emit!(TagCardinalityValueLimitReached { key });
            }

            AcceptResult::Accepted
        } else {
            // New tag value is rejected.
            AcceptResult::Rejected
        }
    }

    /// Checks if recording a key and value corresponding to a tag on an incoming Metric would
    /// exceed the cardinality limit.
    fn tag_limit_exceeded(
        &self,
        metric_key: Option<&MetricId>,
        key: &str,
        value: &TagValueSet,
    ) -> bool {
        self.accepted_tags
            .get(&metric_key.cloned())
            .and_then(|metric_accepted_tags| {
                metric_accepted_tags.get(key).map(|value_set| {
                    !value_set.contains(value)
                        && value_set.len() >= self.get_config_for_metric(metric_key).value_limit
                })
            })
            .unwrap_or(false)
    }

    /// Record a key and value corresponding to a tag on an incoming Metric.
    ///
    /// Returns `true` if the (metric, tag-key) pair could not be allocated due to
    /// `max_tracked_keys` (and therefore the value is not recorded), `false` otherwise.
    fn record_tag_value(
        &mut self,
        metric_key: Option<&MetricId>,
        key: &str,
        value: &TagValueSet,
    ) -> bool {
        let config = *self.get_config_for_metric(metric_key);
        let metric_key_owned = metric_key.cloned();

        let pair_exists = self
            .accepted_tags
            .get(&metric_key_owned)
            .is_some_and(|m| m.contains_key(key));

        if !pair_exists {
            if !self.can_allocate_new_key() {
                return true;
            }
            self.record_new_key_allocation();
        }

        let metric_accepted_tags = self.accepted_tags.entry(metric_key_owned).or_default();
        metric_accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(config.value_limit, &config.mode))
            .insert(value.clone());
        false
    }

    pub fn transform_one(&mut self, mut event: Event) -> Option<Event> {
        let metric = event.as_mut_metric();
        let metric_name = metric.name().to_string();
        let metric_namespace = metric.namespace().map(|n| n.to_string());
        let metric_key = match self.config.tracking_scope {
            TrackingScope::PerMetric => Some((metric_namespace, metric_name.clone())),
            TrackingScope::Global => {
                let has_per_metric_config =
                    self.config.per_metric_limits.iter().any(|(name, config)| {
                        *name == metric_name
                            && (config.namespace.is_none() || config.namespace == metric_namespace)
                    });
                if has_per_metric_config {
                    Some((metric_namespace, metric_name.clone()))
                } else {
                    None
                }
            }
        };
        if let Some(tags_map) = metric.tags_mut() {
            let include_extended_tags = self
                .get_config_for_metric(metric_key.as_ref())
                .internal_metrics
                .include_extended_tags;
            let mut any_untracked = false;

            match self
                .get_config_for_metric(metric_key.as_ref())
                .limit_exceeded_action
            {
                LimitExceededAction::DropEvent => {
                    // This needs to check all the tags, to ensure that the ordering of tag names
                    // doesn't change the behavior of the check.

                    for (key, value) in tags_map.iter_sets() {
                        if self.tag_limit_exceeded(metric_key.as_ref(), key, value) {
                            emit!(TagCardinalityLimitRejectingEvent {
                                metric_name: &metric_name,
                                tag_key: key,
                                tag_value: &value.to_string(),
                                include_extended_tags,
                            });
                            return None;
                        }
                    }
                    for (key, value) in tags_map.iter_sets() {
                        if self.record_tag_value(metric_key.as_ref(), key, value) {
                            any_untracked = true;
                        }
                    }
                }
                LimitExceededAction::DropTag => {
                    tags_map.retain(|key, value| {
                        match self.try_accept_tag(metric_key.as_ref(), key, value) {
                            AcceptResult::Accepted => true,
                            AcceptResult::Rejected => {
                                emit!(TagCardinalityLimitRejectingTag {
                                    metric_name: &metric_name,
                                    tag_key: key,
                                    tag_value: &value.to_string(),
                                    include_extended_tags,
                                });
                                false
                            }
                            AcceptResult::Untracked => {
                                any_untracked = true;
                                true // pass through unchecked
                            }
                        }
                    });
                }
            }

            if any_untracked {
                emit!(TagCardinalityLimitUntracked);
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
