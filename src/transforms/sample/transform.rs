use std::collections::HashMap;
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

#[derive(Clone)]
pub struct Sample {
    name: String,
    rate: u64,
    key_field: Option<String>,
    group_by: Option<Template>,
    exclude: Option<Condition>,
    sample_rate_key: OptionalValuePath,
    counter: HashMap<Option<String>, u64>,
}

impl Sample {
    // This function is dead code when the feature flag `transforms-impl-sample` is specified but not
    // `transforms-sample`.
    #![allow(dead_code)]
    pub fn new(
        name: String,
        rate: u64,
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
            counter: HashMap::new(),
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

        let counter_value: u64 = *self.counter.entry(group_by_key.clone()).or_default();

        let num = if let Some(value) = value {
            seahash::hash(value.as_bytes())
        } else {
            counter_value
        };

        // reset counter for particular key, or default key if group_by option isn't provided
        let increment: u64 = (counter_value + 1) % self.rate;
        self.counter.insert(group_by_key.clone(), increment);

        if num % self.rate == 0 {
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
