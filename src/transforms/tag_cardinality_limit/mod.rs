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
    BloomFilterConfig, Config, Inner, LimitExceededAction, Mode, OverrideInner, OverrideMode,
    PerMetricConfig, PerTagConfig, PerTagMode, TrackingScope,
};

use tag_value_set::AcceptedTagValueSet;

use crate::event::metric::TagValueSet;

type MetricId = (Option<String>, String);

/// Outcome of applying tag cardinality tracking to a tag value.
#[derive(Debug, Eq, PartialEq)]
enum AcceptResult {
    /// The tag value was tracked and is within the configured `value_limit`,
    /// or the tag is excluded and passes through unconditionally.
    Tracked,
    /// The tag value was tracked and exceeded the configured `value_limit`.
    Dropped,
    /// The tag value was not tracked because tracking capacity is exhausted.
    Untracked,
}

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
    /// Total count of currently-tracked (metric_bucket, tag_key) pairs.
    /// Used to enforce `config.max_tracked_keys`.
    tracked_keys_count: usize,
}

impl TagCardinalityLimit {
    pub fn new(config: Config) -> Self {
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

    /// Resolve the configuration that applies to a specific (metric, tag) pair.
    ///
    /// Per-tag entries support two modes:
    /// - `mode: limit_override` — uses the per-tag `value_limit`; all other settings
    ///   (`mode`, `cache_size_per_key`, `limit_exceeded_action`, `internal_metrics`)
    ///   are inherited from the enclosing per-metric (or, for global overrides, the
    ///   global) config.
    /// - `mode: excluded` — opts the tag out entirely; all values pass through.
    ///
    /// Per-metric exclusion is blanket: `mode: excluded` on a per-metric entry opts out
    /// every tag on that metric and `per_tag_limits` is ignored.
    ///
    /// Per-metric `per_tag_limits` take precedence over the top-level
    /// `Config::per_tag_limits`: when a metric matches a per-metric entry, the global
    /// per-tag overrides are not consulted for that metric.
    fn get_config_for_metric_tag(
        &self,
        metric_key: Option<&MetricId>,
        tag_key: &str,
    ) -> TagSettings {
        // No matching per-metric override → use the global config, with global
        // per-tag overrides layered on top.
        let Some((metric_namespace, metric_name)) = metric_key else {
            return self.apply_global_per_tag(tag_key);
        };
        let Some((_, per_metric)) = self.config.per_metric_limits.iter().find(|(name, cfg)| {
            *name == metric_name && (cfg.namespace.is_none() || cfg.namespace == *metric_namespace)
        }) else {
            return self.apply_global_per_tag(tag_key);
        };

        // Per-metric exclusion is blanket — per-tag overrides do not apply.
        let Some(metric_mode) = per_metric.config.mode.as_mode() else {
            return TagSettings::Excluded;
        };
        let limit_exceeded_action = per_metric.config.limit_exceeded_action;
        let metric_value_limit = per_metric.config.value_limit;
        let internal_metrics = per_metric.config.internal_metrics;

        // Per-tag entry: LimitOverride uses an explicit value_limit; Excluded opts
        // the tag out. All other settings are always inherited from per-metric.
        if let Some(per_tag) = per_metric.per_tag_limits.get(tag_key) {
            match per_tag.mode {
                PerTagMode::Excluded => return TagSettings::Excluded,
                PerTagMode::LimitOverride { value_limit } => {
                    // Tracking algorithm and all other settings are always inherited
                    // from the per-metric config.
                    return TagSettings::Tracked(Inner {
                        value_limit,
                        limit_exceeded_action,
                        mode: metric_mode,
                        internal_metrics,
                    });
                }
            }
        }
        TagSettings::Tracked(Inner {
            value_limit: metric_value_limit,
            limit_exceeded_action,
            mode: metric_mode,
            internal_metrics,
        })
    }

    /// Apply the top-level `per_tag_limits` (if any) on top of the global `Inner`.
    /// Used for metrics that do not match any `per_metric_limits` entry.
    fn apply_global_per_tag(&self, tag_key: &str) -> TagSettings {
        let global = self.config.global;
        match self.config.per_tag_limits.get(tag_key).map(|c| c.mode) {
            Some(PerTagMode::Excluded) => TagSettings::Excluded,
            Some(PerTagMode::LimitOverride { value_limit }) => TagSettings::Tracked(Inner {
                value_limit,
                ..global
            }),
            None => TagSettings::Tracked(global),
        }
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

    /// Attempts to accept a tag value for a (metric, tag-key) pair.
    ///
    /// Returns:
    /// - `Tracked` if the value is already tracked, fits under the configured
    ///   `value_limit` and is now recorded, or the per-tag entry is `mode: excluded`
    ///   (pass-through).
    /// - `Dropped` if the value would exceed `value_limit`; the caller should drop
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
        let config = match self.get_config_for_metric_tag(metric_key, key) {
            TagSettings::Excluded => return AcceptResult::Tracked,
            TagSettings::Tracked(inner) => inner,
        };
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
            return AcceptResult::Tracked;
        }

        // Tag value not yet part of the accepted set.
        if tag_value_set.len() < config.value_limit {
            // accept the new value
            tag_value_set.insert(value.clone());

            if tag_value_set.len() == config.value_limit {
                emit!(TagCardinalityValueLimitReached { key });
            }

            AcceptResult::Tracked
        } else {
            // New tag value exceeds the configured limit.
            AcceptResult::Dropped
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
        match self
            .accepted_tags
            .get(&metric_key.cloned())
            .and_then(|metric_accepted_tags| metric_accepted_tags.get(key))
        {
            // Already accepted — never exceeds.
            Some(value_set) if value_set.contains(value) => false,
            // Adding this value would push us at or past the configured cap. Treat a
            // missing bucket as an empty set so `value_limit: 0` correctly rejects
            // the first occurrence too.
            Some(value_set) => value_set.len() >= resolved.value_limit,
            None => resolved.value_limit == 0,
        }
    }

    /// Record an accepted tag value (mutation-only, no limit check). Used by the `DropEvent`
    /// path's record pass after a mutation-free pre-check has confirmed every tag has room.
    /// Excluded tags are skipped — no storage allocated.
    ///
    /// Returns `true` if the (metric, tag-key) pair could not be allocated due to
    /// `max_tracked_keys` (the value is then not recorded). Returns `false` for
    /// successful records and for excluded tags.
    fn record_tag_value(
        &mut self,
        metric_key: Option<&MetricId>,
        key: &str,
        value: &TagValueSet,
    ) -> bool {
        let config = match self.get_config_for_metric_tag(metric_key, key) {
            TagSettings::Excluded => return false,
            TagSettings::Tracked(inner) => inner,
        };
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
            let mut any_untracked = false;

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
                        if self.record_tag_value(metric_key.as_ref(), key, value) {
                            any_untracked = true;
                        }
                    }
                }
                LimitExceededAction::DropTag => {
                    tags_map.retain(|key, value| {
                        match self.try_accept_tag(metric_key.as_ref(), key, value) {
                            AcceptResult::Tracked => true,
                            AcceptResult::Dropped => {
                                let include_extended_tags = match self
                                    .get_config_for_metric_tag(metric_key.as_ref(), key)
                                {
                                    TagSettings::Tracked(inner) => {
                                        inner.internal_metrics.include_extended_tags
                                    }
                                    TagSettings::Excluded => false, // unreachable: excluded tags return Tracked
                                };
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
