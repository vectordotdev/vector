use std::{future::ready, pin::Pin};

use futures::{Stream, StreamExt};
use hashbrown::HashMap;
use vector_lib::{event::Event, transform::TaskTransform};

use crate::internal_events::{
    TagCardinalityLimitRejectingEvent, TagCardinalityLimitRejectingTag,
    TagCardinalityValueLimitReached,
};

pub mod config;
mod tag_value_set;

#[cfg(test)]
mod tests;

pub use config::{BloomFilterConfig, Config, Inner, LimitExceededAction, Mode, PerMetricConfig};
use tag_value_set::AcceptedTagValueSet;

use crate::event::metric::TagValueSet;

type MetricId = (Option<String>, String);

#[derive(Debug)]
pub struct TagCardinalityLimit {
    config: Config,
    accepted_tags: HashMap<Option<MetricId>, HashMap<String, AcceptedTagValueSet>>,
}

impl TagCardinalityLimit {
    fn new(config: Config) -> Self {
        Self {
            config,
            accepted_tags: HashMap::new(),
        }
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

    /// Returns the union of the global `exclude_tags` and the matching per-metric
    /// `exclude_tags`, so global exclusions still apply when a per-metric config matches.
    /// Deduped so that a key listed in both lists is only checked once per tag in the hot path.
    fn effective_exclude_tags(&self, metric_key: Option<&MetricId>) -> Vec<&str> {
        let global = self.config.global.exclude_tags.iter().map(String::as_str);
        let per_metric = metric_key
            .and_then(|id| {
                self.config.per_metric_limits.iter().find(|(name, config)| {
                    **name == id.1 && (config.namespace.is_none() || config.namespace == id.0)
                })
            })
            .into_iter()
            .flat_map(|(_, c)| c.config.exclude_tags.iter().map(String::as_str));

        let mut out: Vec<&str> = Vec::new();
        for k in global.chain(per_metric) {
            if !out.contains(&k) {
                out.push(k);
            }
        }
        out
    }

    /// Takes in key and a value corresponding to a tag on an incoming Metric
    /// Event.  If that value is already part of set of accepted values for that
    /// key, then simply returns true.  If that value is not yet part of the
    /// accepted values for that key, checks whether we have hit the value_limit
    /// for that key yet and if not adds the value to the set of accepted values
    /// for the key and returns true, otherwise returns false.  A false return
    /// value indicates to the caller that the value is not accepted for this
    /// key, and the configured limit_exceeded_action should be taken.
    fn try_accept_tag(
        &mut self,
        metric_key: Option<&MetricId>,
        key: &str,
        value: &TagValueSet,
    ) -> bool {
        // Copy out the `Copy` fields up front to release the borrow on `self.config` before we
        // take `&mut self.accepted_tags` below.
        let (value_limit, mode) = {
            let config = self.get_config_for_metric(metric_key);
            (config.value_limit, config.mode)
        };
        let metric_accepted_tags = self.accepted_tags.entry(metric_key.cloned()).or_default();
        let tag_value_set = metric_accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(value_limit, &mode));

        if tag_value_set.contains(value) {
            // Tag value has already been accepted, nothing more to do.
            return true;
        }

        // Tag value not yet part of the accepted set.
        if tag_value_set.len() < value_limit {
            // accept the new value
            tag_value_set.insert(value.clone());

            if tag_value_set.len() == value_limit {
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
    fn record_tag_value(&mut self, metric_key: Option<&MetricId>, key: &str, value: &TagValueSet) {
        let (value_limit, mode) = {
            let config = self.get_config_for_metric(metric_key);
            (config.value_limit, config.mode)
        };
        let metric_accepted_tags = self.accepted_tags.entry(metric_key.cloned()).or_default();
        metric_accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(value_limit, &mode))
            .insert(value.clone());
    }

    pub fn transform_one(&mut self, mut event: Event) -> Option<Event> {
        let metric = event.as_mut_metric();
        let metric_name = metric.name().to_string();
        let metric_namespace = metric.namespace().map(|n| n.to_string());
        let has_per_metric_config = self.config.per_metric_limits.iter().any(|(name, config)| {
            *name == metric_name
                && (config.namespace.is_none() || config.namespace == metric_namespace)
        });
        let metric_key = if has_per_metric_config {
            Some((metric_namespace, metric_name.clone()))
        } else {
            None
        };
        if let Some(tags_map) = metric.tags_mut() {
            // Materialize as owned strings so the list can outlive the `&mut self` calls in the
            // loops below. Linear scan is faster than a `HashSet` for typical exclusion sizes.
            let excluded_keys: Vec<String> = self
                .effective_exclude_tags(metric_key.as_ref())
                .into_iter()
                .map(str::to_owned)
                .collect();
            let is_excluded = |key: &str| excluded_keys.iter().any(|k| k == key);

            match self
                .get_config_for_metric(metric_key.as_ref())
                .limit_exceeded_action
            {
                LimitExceededAction::DropEvent => {
                    // This needs to check all the tags, to ensure that the ordering of tag names
                    // doesn't change the behavior of the check.

                    for (key, value) in tags_map.iter_sets() {
                        if is_excluded(key) {
                            continue;
                        }
                        if self.tag_limit_exceeded(metric_key.as_ref(), key, value) {
                            let config = self.get_config_for_metric(metric_key.as_ref());
                            emit!(TagCardinalityLimitRejectingEvent {
                                metric_name: &metric_name,
                                tag_key: key,
                                tag_value: &value.to_string(),
                                include_extended_tags: config
                                    .internal_metrics
                                    .include_extended_tags,
                            });
                            return None;
                        }
                    }
                    for (key, value) in tags_map.iter_sets() {
                        if is_excluded(key) {
                            continue;
                        }
                        self.record_tag_value(metric_key.as_ref(), key, value);
                    }
                }
                LimitExceededAction::DropTag => {
                    let config = self.get_config_for_metric(metric_key.as_ref());
                    let include_extended_tags = config.internal_metrics.include_extended_tags;
                    tags_map.retain(|key, value| {
                        if is_excluded(key) {
                            return true;
                        }
                        if self.try_accept_tag(metric_key.as_ref(), key, value) {
                            true
                        } else {
                            emit!(TagCardinalityLimitRejectingTag {
                                metric_name: &metric_name,
                                tag_key: key,
                                tag_value: &value.to_string(),
                                include_extended_tags,
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
