use std::{collections::HashMap, fmt};
use vector_lib::config::LegacyKey;

use crate::{
    conditions::Condition,
    event::Event,
    internal_events::SampleEventDiscarded,
    sinks::prelude::TemplateRenderingError,
    template::Template,
    transforms::{FunctionTransform, OutputBuffer},
};
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vector_lib::lookup::OwnedTargetPath;

/// Exists only for backwards compatability purposes so that the value of sample_rate_key is
/// consistent after the internal implementation of the Sample class was modified to work in terms
/// of percentages
#[derive(Clone, Debug)]
pub enum SampleMode {
    Rate(u64, HashMap<Option<String>, u64>),
    Ratio(f64, HashMap<Option<String>, f64>),
}

impl SampleMode {
    pub fn new_rate(rate: u64) -> Self {
        Self::Rate(rate, HashMap::default())
    }

    pub fn new_ratio(ratio: f64) -> Self {
        Self::Ratio(ratio, HashMap::default())
    }

    fn increment(&mut self, key: &Option<String>) -> bool {
        match self {
            Self::Rate(rate, counter) => {
                let counter_value = counter.entry(key.clone()).or_default();
                let increment: u64 = *counter_value + 1;
                *counter_value = increment;
                increment % *rate == 0
            }
            Self::Ratio(ratio, counter) => {
                let counter_value = counter.entry(key.clone()).or_insert(*ratio);
                let increment: f64 = *counter_value + *ratio;
                *counter_value = if increment >= 1.0 {
                    increment - 1.0
                } else {
                    increment
                };
                increment >= 1.0
            }
        }
    }

    fn hash_within_ratio(&self, value: &[u8]) -> bool {
        let hash = seahash::hash(value);
        match self {
            Self::Rate(rate, _) => hash % rate == 0,
            Self::Ratio(ratio, _) => {
                // Assuming an even distribution of values, process the event if the value of its hash %
                // 100, is within the allowable configured ratio
                (hash % 100) as f64 <= (ratio * 100.0)
            }
        }
    }
}

impl fmt::Display for SampleMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Avoids the print of an additional '.0' which was not performed in the previous
        // implementation
        match self {
            Self::Rate(integer_rate, _) => write!(f, "{integer_rate}"),
            Self::Ratio(percent_rate, _) => write!(f, "{percent_rate}"),
        }
    }
}

#[derive(Clone)]
pub struct Sample {
    name: String,
    rate: SampleMode,
    key_field: Option<String>,
    group_by: Option<Template>,
    exclude: Option<Condition>,
    sample_rate_key: OptionalValuePath,
}

impl Sample {
    // This function is dead code when the feature flag `transforms-impl-sample` is specified but not
    // `transforms-sample`.
    #![allow(dead_code)]
    pub fn new(
        name: String,
        rate: SampleMode,
        key_field: Option<String>,
        group_by: Option<Template>,
        exclude: Option<Condition>,
        sample_rate_key: OptionalValuePath,
    ) -> Self {
        Self {
            name,
            rate,
            key_field,
            group_by,
            exclude,
            sample_rate_key,
        }
    }

    pub fn ratio(&self) -> f64 {
        match self.rate {
            SampleMode::Rate(rate, _) => 1.0f64 / rate as f64,
            SampleMode::Ratio(ratio, _) => ratio,
        }
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

        let value = self
            .key_field
            .as_ref()
            .and_then(|key_field| match &event {
                Event::Log(event) => event
                    .parse_path_and_get_value(key_field.as_str())
                    .ok()
                    .flatten(),
                Event::Trace(event) => event
                    .parse_path_and_get_value(key_field.as_str())
                    .ok()
                    .flatten(),
                Event::Metric(_) => panic!("component can never receive metric events"),
            })
            .map(|v| v.to_string_lossy());

        // Fetch actual field value if group_by option is set.
        let group_by_key = self.group_by.as_ref().and_then(|group_by| match &event {
            Event::Log(event) => group_by
                .render_string(event)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("group_by"),
                        drop_event: false,
                    })
                })
                .ok(),
            Event::Trace(event) => group_by
                .render_string(event)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("group_by"),
                        drop_event: false,
                    })
                })
                .ok(),
            Event::Metric(_) => panic!("component can never receive metric events"),
        });

        let threshold_exceeded = self.rate.increment(&group_by_key);
        let should_process = if let Some(value) = value {
            self.rate.hash_within_ratio(value.as_bytes())
        } else {
            threshold_exceeded
        };

        if should_process {
            if let Some(path) = &self.sample_rate_key.path {
                match event {
                    Event::Log(ref mut event) => {
                        event.namespace().insert_source_metadata(
                            self.name.as_str(),
                            event,
                            Some(LegacyKey::Overwrite(path)),
                            path,
                            self.rate.to_string(),
                        );
                    }
                    Event::Trace(ref mut event) => {
                        event.insert(&OwnedTargetPath::event(path.clone()), self.rate.to_string());
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
