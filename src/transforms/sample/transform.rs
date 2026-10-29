use std::{fmt, mem, num::NonZeroU64};

use lru::LruCache;
use vector_lib::{
    ByteSizeOf,
    config::LegacyKey,
    lookup::{OwnedTargetPath, lookup_v2::OptionalValuePath},
    sampling::RatioSampler,
};

use crate::{
    conditions::Condition,
    event::{Event, Value},
    internal_events::SampleEventDiscarded,
    sinks::prelude::TemplateRenderingError,
    template::UnconfinedTemplate,
    transforms::{FunctionTransform, OutputBuffer},
};

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Preserve existing numeric conversion behavior. https://github.com/vectordotdev/vector/issues/23659"
)]
fn hash_ratio_threshold(ratio: f64) -> u64 {
    (ratio * u128::from(u64::MAX) as f64) as u64
}

/// Approximate bytes retained for one cached group: the key (including its heap allocation), the
/// sampling state, the LRU node's previous/next pointers, and the hash table slot pointing at it.
#[expect(
    clippy::ref_option,
    reason = "Measure the owned Option and its heap allocation."
)]
pub(super) fn group_state_entry_bytes(group_by_key: &Option<String>) -> usize {
    group_by_key.size_of() + mem::size_of::<GroupState>() + 4 * mem::size_of::<usize>()
}

/// Exists only for backwards compatibility purposes so that the value of `sample_rate_key` is
/// consistent after the internal implementation of the Sample class was modified to work in terms
/// of percentages
#[derive(Clone, Debug)]
pub enum SampleMode {
    Rate {
        rate: u64,
    },
    Ratio {
        ratio: f64,
        hash_ratio_threshold: u64,
    },
}

impl SampleMode {
    #[must_use]
    pub const fn new_rate(rate: u64) -> Self {
        Self::Rate { rate }
    }

    #[must_use]
    pub fn new_ratio(ratio: f64) -> Self {
        Self::Ratio {
            ratio,
            // Supports the 'key_field' option, assuming an equal distribution of values for a given
            // field, hashing its contents this component should output events according to the
            // configured ratio.
            //
            // To do one option would be to convert the hash to a number between 0 and 1 and compare
            // to the ratio. However to address issues with precision, here the ratio is scaled to
            // meet the width of the type of the hash.
            hash_ratio_threshold: hash_ratio_threshold(ratio),
        }
    }

    fn new_state(&self) -> StaticSampleState {
        match self {
            Self::Rate { rate } => StaticSampleState::Rate {
                rate: *rate,
                counter: 0,
            },
            Self::Ratio { ratio, .. } => StaticSampleState::Ratio {
                sampler: RatioSampler::new(*ratio),
            },
        }
    }

    fn hash_within_ratio(&self, value: &[u8]) -> bool {
        let hash = seahash::hash(value);
        match self {
            Self::Rate { rate } => hash.is_multiple_of(*rate),
            Self::Ratio {
                hash_ratio_threshold,
                ..
            } => hash <= *hash_ratio_threshold,
        }
    }
}

#[derive(Clone, Debug)]
enum StaticSampleState {
    Rate { rate: u64, counter: u64 },
    Ratio { sampler: RatioSampler },
}

impl StaticSampleState {
    fn sample(&mut self) -> bool {
        match self {
            Self::Rate { rate, counter } => {
                let old_counter_value = *counter;
                *counter += 1;
                old_counter_value % *rate == 0
            }
            Self::Ratio { sampler } => sampler.sample(),
        }
    }
}

#[derive(Clone, Debug)]
struct GroupState {
    static_state: StaticSampleState,
    dynamic_event_counter: u64,
}

impl GroupState {
    fn take_next(&mut self) -> Self {
        let next = self.clone();
        self.static_state.sample();
        self.dynamic_event_counter += 1;
        next
    }
}

enum EventSampleMode {
    Ratio(f64),
    Rate(NonZeroU64),
}

impl EventSampleMode {
    fn sample_rate_label(&self) -> String {
        match self {
            Self::Ratio(ratio) => ratio.to_string(),
            Self::Rate(rate) => rate.to_string(),
        }
    }

    fn sample(&self, counter: &mut u64) -> bool {
        let old_counter_value = *counter;
        *counter += 1;
        let hash = seahash::hash(&old_counter_value.to_ne_bytes());

        match self {
            Self::Ratio(ratio) => hash <= hash_ratio_threshold(*ratio),
            Self::Rate(rate) => hash.is_multiple_of(rate.get()),
        }
    }
}

#[derive(Clone, Default)]
pub struct DynamicSampleFields {
    pub ratio_field: Option<String>,
    pub rate_field: Option<String>,
}

impl fmt::Display for SampleMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Avoids the print of an additional '.0' which was not performed in the previous
        // implementation
        match self {
            Self::Rate { rate, .. } => write!(f, "{rate}"),
            Self::Ratio { ratio, .. } => write!(f, "{ratio}"),
        }
    }
}

#[derive(Clone)]
pub enum SampleKeySource {
    Static {
        key_field: Option<String>,
        group_by: Option<UnconfinedTemplate>,
    },
    Dynamic {
        fields: DynamicSampleFields,
        group_by: Option<UnconfinedTemplate>,
    },
}

impl SampleKeySource {
    const fn group_by(&self) -> Option<&UnconfinedTemplate> {
        match self {
            Self::Static { group_by, .. } | Self::Dynamic { group_by, .. } => group_by.as_ref(),
        }
    }
}

#[derive(Clone)]
pub struct Sample {
    name: String,
    static_mode: SampleMode,
    key_source: SampleKeySource,
    next_group_state: GroupState,
    group_states: LruCache<Option<String>, GroupState>,
    group_state_bytes: usize,
    max_state_bytes: usize,
    exclude: Option<Condition>,
    rate_key: OptionalValuePath,
    include_group_tag: bool,
}

impl Sample {
    // This function is dead code when the feature flag `transforms-impl-sample` is specified but not
    // `transforms-sample`.
    #![allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        name: String,
        static_mode: SampleMode,
        key_field: Option<String>,
        group_by: Option<UnconfinedTemplate>,
        max_state_bytes: usize,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
        include_group_tag: bool,
    ) -> Self {
        Self::new_with_source(
            name,
            static_mode,
            SampleKeySource::Static {
                key_field,
                group_by,
            },
            max_state_bytes,
            exclude,
            sample_rate_key,
            include_group_tag,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new_with_dynamic(
        name: String,
        static_mode: SampleMode,
        fields: DynamicSampleFields,
        group_by: Option<UnconfinedTemplate>,
        max_state_bytes: usize,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
        include_group_tag: bool,
    ) -> Self {
        Self::new_with_source(
            name,
            static_mode,
            SampleKeySource::Dynamic { fields, group_by },
            max_state_bytes,
            exclude,
            sample_rate_key,
            include_group_tag,
        )
    }

    fn new_with_source(
        name: String,
        static_mode: SampleMode,
        key_source: SampleKeySource,
        max_state_bytes: usize,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
        include_group_tag: bool,
    ) -> Self {
        let next_group_state = GroupState {
            static_state: static_mode.new_state(),
            dynamic_event_counter: 0,
        };
        Self {
            name,
            static_mode,
            key_source,
            next_group_state,
            group_states: LruCache::unbounded(),
            group_state_bytes: 0,
            max_state_bytes,
            exclude,
            rate_key: sample_rate_key,
            include_group_tag,
        }
    }

    /// Runs `sample` against the group's state, admitting unknown groups from the shared
    /// admission state and evicting least recently used groups to stay within `max_state_bytes`.
    ///
    /// A group whose entry alone exceeds the limit is sampled but not retained, so each of its
    /// events is admitted like a new group.
    fn sample_group(
        &mut self,
        group_by_key: Option<String>,
        sample: impl FnOnce(&mut GroupState) -> bool,
    ) -> bool {
        if let Some(state) = self.group_states.get_mut(&group_by_key) {
            return sample(state);
        }

        let mut state = self.next_group_state.take_next();
        let should_sample = sample(&mut state);

        let entry_bytes = group_state_entry_bytes(&group_by_key);
        if entry_bytes <= self.max_state_bytes {
            while self.group_state_bytes + entry_bytes > self.max_state_bytes {
                let Some((evicted_key, _)) = self.group_states.pop_lru() else {
                    break;
                };
                self.group_state_bytes -= group_state_entry_bytes(&evicted_key);
            }
            self.group_states.put(group_by_key, state);
            self.group_state_bytes += entry_bytes;
        }

        should_sample
    }

    #[cfg(test)]
    pub const fn retained_state_bytes(&self) -> usize {
        self.group_state_bytes
    }

    #[cfg(test)]
    #[allow(
        clippy::cast_precision_loss,
        reason = "Preserve existing numeric conversion behavior. https://github.com/vectordotdev/vector/issues/23659"
    )]
    pub fn ratio(&self) -> f64 {
        match &self.static_mode {
            SampleMode::Rate { rate, .. } => 1.0f64 / *rate as f64,
            SampleMode::Ratio { ratio, .. } => *ratio,
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "Preserve existing numeric conversion behavior. https://github.com/vectordotdev/vector/issues/23659"
    )]
    fn event_ratio(&self, event: &Event) -> Option<f64> {
        let ratio_field = match &self.key_source {
            SampleKeySource::Dynamic { fields, .. } => fields.ratio_field.as_ref()?,
            SampleKeySource::Static { .. } => return None,
        };

        let value = Self::get_event_value(event, ratio_field.as_str())?;

        let ratio = match value {
            Value::Integer(value) => *value as f64,
            Value::Float(value) => value.into_inner(),
            Value::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.parse::<f64>().ok()?,
            Value::String(string) => string.parse::<f64>().ok()?,
            _ => return None,
        };

        (ratio > 0.0 && ratio <= 1.0).then_some(ratio)
    }

    fn event_rate(&self, event: &Event) -> Option<NonZeroU64> {
        let rate_field = match &self.key_source {
            SampleKeySource::Dynamic { fields, .. } => fields.rate_field.as_ref()?,
            SampleKeySource::Static { .. } => return None,
        };

        let value = Self::get_event_value(event, rate_field.as_str())?;

        match value {
            Value::Integer(value) => u64::try_from(*value).ok().and_then(NonZeroU64::new),
            Value::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.parse::<NonZeroU64>().ok(),
            Value::String(string) => string.parse::<NonZeroU64>().ok(),
            _ => None,
        }
    }

    fn get_event_value<'a>(event: &'a Event, path: &str) -> Option<&'a Value> {
        match event {
            Event::Log(event) => event.parse_path_and_get_value(path).ok().flatten(),
            Event::Trace(event) => event.parse_path_and_get_value(path).ok().flatten(),
            Event::Metric(_) => panic!("component can never receive metric events"),
        }
    }

    fn event_sample_mode(&self, event: &Event) -> Option<EventSampleMode> {
        self.event_ratio(event)
            .map(EventSampleMode::Ratio)
            .or_else(|| self.event_rate(event).map(EventSampleMode::Rate))
    }

    fn group_by_key(&self, event: &Event) -> Option<String> {
        let group_by = self.key_source.group_by()?;

        match event {
            Event::Log(event) => group_by.render_string(event),
            Event::Trace(event) => group_by.render_string(event),
            Event::Metric(_) => panic!("component can never receive metric events"),
        }
        .map_err(|error| {
            emit!(TemplateRenderingError {
                error,
                field: Some("group_by"),
                drop_event: false,
            });
        })
        .ok()
    }

    fn static_key_value<'a>(&self, event: &'a Event) -> Option<&'a Value> {
        let key_field = match &self.key_source {
            SampleKeySource::Static { key_field, .. } => key_field.as_ref()?,
            SampleKeySource::Dynamic { .. } => return None,
        };

        Self::get_event_value(event, key_field)
    }
}

impl FunctionTransform for Sample {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let mut event = {
            if let Some(condition) = self.exclude.as_ref() {
                let (result, event) = condition.check(event);
                if result {
                    output.push(event);
                    return;
                }
                event
            } else {
                event
            }
        };

        let group_by_key = self.group_by_key(&event);
        let discarded_group = self
            .include_group_tag
            .then(|| group_by_key.clone())
            .flatten();
        let static_key_sample = self.static_key_value(&event).map(|value| {
            self.static_mode
                .hash_within_ratio(value.to_string_lossy().as_bytes())
        });
        let event_sample_mode = self.event_sample_mode(&event);
        let sample_rate = event_sample_mode.as_ref().map_or_else(
            || self.static_mode.to_string(),
            EventSampleMode::sample_rate_label,
        );

        let should_sample = self.sample_group(group_by_key, |group_state| {
            if let Some(mode) = event_sample_mode {
                mode.sample(&mut group_state.dynamic_event_counter)
            } else {
                let threshold_exceeded = group_state.static_state.sample();
                static_key_sample.unwrap_or(threshold_exceeded)
            }
        });

        if should_sample {
            if let Some(path) = &self.rate_key.path {
                match event {
                    Event::Log(ref mut event) => {
                        event.namespace().insert_source_metadata(
                            self.name.as_str(),
                            event,
                            Some(LegacyKey::Overwrite(path)),
                            path,
                            sample_rate,
                        );
                    }
                    Event::Trace(ref mut event) => {
                        event.insert(&OwnedTargetPath::event(path.clone()), sample_rate);
                    }
                    Event::Metric(_) => panic!("component can never receive metric events"),
                }
            }
            output.push(event);
        } else {
            emit!(SampleEventDiscarded {
                group: discarded_group,
                include_group_tag: self.include_group_tag,
            });
        }
    }
}
