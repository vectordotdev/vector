use std::{
    fmt,
    num::{NonZeroU64, NonZeroUsize},
};

use lru::LruCache;
use vector_lib::{
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

fn hash_ratio_threshold(ratio: f64) -> u64 {
    (ratio * (u64::MAX as u128) as f64) as u64
}

/// Exists only for backwards compatibility purposes so that the value of sample_rate_key is
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
    pub const fn new_rate(rate: u64) -> Self {
        Self::Rate { rate }
    }

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
    exclude: Option<Condition>,
    sample_rate_key: OptionalValuePath,
}

impl Sample {
    // This function is dead code when the feature flag `transforms-impl-sample` is specified but not
    // `transforms-sample`.
    #![allow(dead_code)]
    pub fn new(
        name: String,
        static_mode: SampleMode,
        key_field: Option<String>,
        group_by: Option<UnconfinedTemplate>,
        max_groups: NonZeroUsize,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
    ) -> Self {
        Self::new_with_source(
            name,
            static_mode,
            SampleKeySource::Static {
                key_field,
                group_by,
            },
            max_groups,
            exclude,
            sample_rate_key,
        )
    }

    pub fn new_with_dynamic(
        name: String,
        static_mode: SampleMode,
        fields: DynamicSampleFields,
        group_by: Option<UnconfinedTemplate>,
        max_groups: NonZeroUsize,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
    ) -> Self {
        Self::new_with_source(
            name,
            static_mode,
            SampleKeySource::Dynamic { fields, group_by },
            max_groups,
            exclude,
            sample_rate_key,
        )
    }

    fn new_with_source(
        name: String,
        static_mode: SampleMode,
        key_source: SampleKeySource,
        max_groups: NonZeroUsize,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
    ) -> Self {
        let next_group_state = GroupState {
            static_state: static_mode.new_state(),
            dynamic_event_counter: 0,
        };
        let group_capacity = if key_source.group_by().is_some() {
            max_groups
        } else {
            NonZeroUsize::MIN
        };
        let mut group_states = LruCache::unbounded();
        group_states.resize(group_capacity);
        Self {
            name,
            static_mode,
            key_source,
            next_group_state,
            group_states,
            exclude,
            sample_rate_key,
        }
    }

    fn group_state(&mut self, group_by_key: Option<String>) -> &mut GroupState {
        let next_group_state = &mut self.next_group_state;
        self.group_states
            .get_or_insert_mut(group_by_key, || next_group_state.take_next())
    }

    #[cfg(test)]
    pub fn ratio(&self) -> f64 {
        match &self.static_mode {
            SampleMode::Rate { rate, .. } => 1.0f64 / *rate as f64,
            SampleMode::Ratio { ratio, .. } => *ratio,
        }
    }

    fn event_ratio(&self, event: &Event) -> Option<f64> {
        let ratio_field = match &self.key_source {
            SampleKeySource::Dynamic { fields, .. } => fields.ratio_field.as_ref()?,
            SampleKeySource::Static { .. } => return None,
        };

        let value = self.get_event_value(event, ratio_field.as_str())?;

        let ratio = match value {
            Value::Integer(value) => *value as f64,
            Value::Float(value) => value.into_inner(),
            Value::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.parse::<f64>().ok()?,
            _ => return None,
        };

        (ratio > 0.0 && ratio <= 1.0).then_some(ratio)
    }

    fn event_rate(&self, event: &Event) -> Option<NonZeroU64> {
        let rate_field = match &self.key_source {
            SampleKeySource::Dynamic { fields, .. } => fields.rate_field.as_ref()?,
            SampleKeySource::Static { .. } => return None,
        };

        let value = self.get_event_value(event, rate_field.as_str())?;

        match value {
            Value::Integer(value) => u64::try_from(*value).ok().and_then(NonZeroU64::new),
            Value::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.parse::<NonZeroU64>().ok(),
            _ => None,
        }
    }

    fn get_event_value<'a>(&self, event: &'a Event, path: &str) -> Option<&'a Value> {
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
            })
        })
        .ok()
    }

    fn static_key_value<'a>(&self, event: &'a Event) -> Option<&'a Value> {
        let key_field = match &self.key_source {
            SampleKeySource::Static { key_field, .. } => key_field.as_ref()?,
            SampleKeySource::Dynamic { .. } => return None,
        };

        self.get_event_value(event, key_field)
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
                } else {
                    event
                }
            } else {
                event
            }
        };

        let group_by_key = self.group_by_key(&event);
        let static_key_sample = self.static_key_value(&event).map(|value| {
            self.static_mode
                .hash_within_ratio(value.to_string_lossy().as_bytes())
        });
        let event_sample_mode = self.event_sample_mode(&event);
        let sample_rate = event_sample_mode
            .as_ref()
            .map(EventSampleMode::sample_rate_label)
            .unwrap_or_else(|| self.static_mode.to_string());

        let group_state = self.group_state(group_by_key);
        let should_sample = match event_sample_mode {
            Some(mode) => mode.sample(&mut group_state.dynamic_event_counter),
            None => {
                let threshold_exceeded = group_state.static_state.sample();
                static_key_sample.unwrap_or(threshold_exceeded)
            }
        };

        if should_sample {
            if let Some(path) = &self.sample_rate_key.path {
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
                };
            }
            output.push(event);
        } else {
            emit!(SampleEventDiscarded);
        }
    }
}
