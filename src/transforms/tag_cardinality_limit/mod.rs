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

pub use config::{
    BloomFilterConfig, Config, Inner, LimitExceededAction, Mode, OverrideInner, OverrideMode,
    PerMetricConfig, PerTagConfig, PerTagInner, TrackingScope,
};
use tag_value_set::AcceptedTagValueSet;

use crate::event::metric::TagValueSet;

type MetricId = (Option<String>, String);

/// Tag tracking settings for a single (metric, tag) pair.
enum TagSettings {
    /// The tag is excluded from cardinality control; pass values through unchanged.
    Excluded,
    /// The tag is tracked using these settings.
    Tracked(Inner),
}

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

    /// Resolve the configuration that applies to a specific (metric, tag) pair.
    ///
    /// Lookup chain (per field):
    /// - `value_limit`, `mode`, `internal_metrics`: per-tag override → per-metric override → global.
    /// - `limit_exceeded_action`: per-metric override → global. Per-tag entries always inherit
    ///   the action from the enclosing per-metric (or global) config.
    ///
    /// Per-metric exclusion is blanket: if the matching per-metric entry has `mode: excluded`,
    /// every tag on the metric is excluded and `per_tag_limits` is ignored.
    fn get_config_for_metric_tag(
        &self,
        metric_key: Option<&MetricId>,
        tag_key: &str,
    ) -> TagSettings {
        // No matching per-metric override → use the global config as-is.
        let Some((metric_namespace, metric_name)) = metric_key else {
            return TagSettings::Tracked(self.config.global);
        };
        let Some((_, per_metric)) = self.config.per_metric_limits.iter().find(|(name, cfg)| {
            *name == metric_name && (cfg.namespace.is_none() || cfg.namespace == *metric_namespace)
        }) else {
            return TagSettings::Tracked(self.config.global);
        };

        // Per-metric exclusion is blanket — per-tag overrides do not apply.
        let Some(metric_mode) = per_metric.config.mode.as_mode() else {
            return TagSettings::Excluded;
        };
        let limit_exceeded_action = per_metric.config.limit_exceeded_action;

        // Per-tag override may further exclude a specific tag, replace `mode`,
        // or replace `value_limit` (unset `value_limit` inherits from the enclosing
        // per-metric config).
        if let Some(per_tag) = per_metric.per_tag_limits.get(tag_key) {
            let Some(mode) = per_tag.config.mode.as_mode() else {
                return TagSettings::Excluded;
            };
            return TagSettings::Tracked(Inner {
                value_limit: per_tag
                    .config
                    .value_limit
                    .unwrap_or(per_metric.config.value_limit),
                limit_exceeded_action,
                mode,
                internal_metrics: per_metric.config.internal_metrics,
            });
        }
        TagSettings::Tracked(Inner {
            value_limit: per_metric.config.value_limit,
            limit_exceeded_action,
            mode: metric_mode,
            internal_metrics: per_metric.config.internal_metrics,
        })
    }

    /// Returns the `limit_exceeded_action` that applies to this metric. Decided once per event:
    /// per-metric override if any, else global.
    fn metric_action(&self, metric_key: Option<&MetricId>) -> LimitExceededAction {
        if let Some(id) = metric_key
            && let Some((_, pmc)) =
                self.config.per_metric_limits.iter().find(|(name, c)| {
                    **name == id.1 && (c.namespace.is_none() || c.namespace == id.0)
                })
        {
            return pmc.config.limit_exceeded_action;
        }
        self.config.global.limit_exceeded_action
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
        let config = match self.get_config_for_metric_tag(metric_key, key) {
            TagSettings::Excluded => return true,
            TagSettings::Tracked(inner) => inner,
        };
        let metric_accepted_tags = self.accepted_tags.entry(metric_key.cloned()).or_default();
        let tag_value_set = metric_accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(config.value_limit, &config.mode));

        if tag_value_set.contains(value) {
            // Tag value has already been accepted, nothing more to do.
            return true;
        }

        // Tag value not yet part of the accepted set.
        if tag_value_set.len() < config.value_limit {
            // accept the new value
            tag_value_set.insert(value.clone());

            if tag_value_set.len() == config.value_limit {
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
        let resolved = match self.get_config_for_metric_tag(metric_key, key) {
            TagSettings::Excluded => return false,
            TagSettings::Tracked(inner) => inner,
        };
        self.accepted_tags
            .get(&metric_key.cloned())
            .and_then(|metric_accepted_tags| {
                metric_accepted_tags.get(key).map(|value_set| {
                    !value_set.contains(value) && value_set.len() >= resolved.value_limit
                })
            })
            .unwrap_or(false)
    }

    /// Record an accepted tag value (mutation-only, no limit check). Used by the `DropEvent`
    /// path's record pass after a mutation-free pre-check has confirmed every tag has room.
    /// Excluded tags are skipped — no storage allocated.
    fn record_tag_value(&mut self, metric_key: Option<&MetricId>, key: &str, value: &TagValueSet) {
        let config = match self.get_config_for_metric_tag(metric_key, key) {
            TagSettings::Excluded => return,
            TagSettings::Tracked(inner) => inner,
        };
        let metric_accepted_tags = self.accepted_tags.entry(metric_key.cloned()).or_default();
        metric_accepted_tags
            .entry_ref(key)
            .or_insert_with(|| AcceptedTagValueSet::new(config.value_limit, &config.mode))
            .insert(value.clone());
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
            match self.metric_action(metric_key.as_ref()) {
                LimitExceededAction::DropEvent => {
                    // This needs to check all the tags, to ensure that the ordering of tag
                    // names doesn't change the behavior of the check.
                    for (key, value) in tags_map.iter_sets() {
                        let TagSettings::Tracked(resolved) =
                            self.get_config_for_metric_tag(metric_key.as_ref(), key)
                        else {
                            continue; // excluded tags can never trigger DropEvent
                        };
                        if self.tag_limit_exceeded(metric_key.as_ref(), key, value) {
                            let include_extended_tags =
                                resolved.internal_metrics.include_extended_tags;
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
                        self.record_tag_value(metric_key.as_ref(), key, value);
                    }
                }
                LimitExceededAction::DropTag => {
                    tags_map.retain(|key, value| {
                        if self.try_accept_tag(metric_key.as_ref(), key, value) {
                            true
                        } else {
                            let include_extended_tags =
                                match self.get_config_for_metric_tag(metric_key.as_ref(), key) {
                                    TagSettings::Tracked(inner) => {
                                        inner.internal_metrics.include_extended_tags
                                    }
                                    TagSettings::Excluded => false, // unreachable: excluded tags accept
                                };
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
