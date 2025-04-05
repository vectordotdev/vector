use std::{borrow::Cow, collections::HashMap, fmt};
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
    Rate {
        rate: u64,
        counters: HashMap<Option<String>, u64>,
    },
    Ratio {
        ratio: f64,
        values: HashMap<Option<String>, f64>,
        hash_ratio_threshold: u64,
    },
}

impl SampleMode {
    pub fn new_rate(rate: u64) -> Self {
        Self::Rate {
            rate,
            counters: HashMap::default(),
        }
    }

    pub fn new_ratio(ratio: f64) -> Self {
        Self::Ratio {
            ratio,
            values: HashMap::default(),
            // Supports the 'key_field' option, assuming an equal distribution of values for a given
            // field, hashing its contents this component should output events according to the
            // configured ratio.
            //
            // To do one option would be to convert the hash to a number between 0 and 1 and compare
            // to the ratio. However to address issues with precision, here the ratio is scaled to
            // meet the width of the type of the hash.
            hash_ratio_threshold: (ratio * (u64::MAX as u128) as f64) as u64,
        }
    }

    fn increment(&mut self, group_by_key: &Option<String>, value: &Option<Cow<'_, str>>) -> bool {
        let threshold_exceeded = match self {
            Self::Rate { rate, counters } => {
                let counter_value = counters.entry(group_by_key.clone()).or_default();
                let old_counter_value = *counter_value;
                *counter_value += 1;
                old_counter_value % *rate == 0
            }
            Self::Ratio { ratio, values, .. } => {
                let value = values.entry(group_by_key.clone()).or_insert(1.0 - *ratio);
                let increment: f64 = *value + *ratio;
                *value = if increment >= 1.0 {
                    increment - 1.0
                } else {
                    increment
                };
                increment >= 1.0
            }
        };
        if let Some(value) = value {
            self.hash_within_ratio(value.as_bytes())
        } else {
            threshold_exceeded
        }
    }

    fn hash_within_ratio(&self, value: &[u8]) -> bool {
        let hash = seahash::hash(value);
        match self {
            Self::Rate { rate, .. } => hash % rate == 0,
            Self::Ratio {
                hash_ratio_threshold,
                ..
            } => hash <= *hash_ratio_threshold,
        }
    }
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
    pub const fn new(
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

    #[cfg(test)]
    pub fn ratio(&self) -> f64 {
        match self.rate {
            SampleMode::Rate { rate, .. } => 1.0f64 / rate as f64,
            SampleMode::Ratio { ratio, .. } => ratio,
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

        let should_sample = self.rate.increment(&group_by_key, &value);
        if should_sample {
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
