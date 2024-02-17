use vector_lib::config::LegacyKey;
use vrl::event_path;

use crate::{
    conditions::Condition,
    event::Event,
    internal_events::SampleEventDiscarded,
    transforms::{FunctionTransform, OutputBuffer},
};

#[derive(Clone)]
pub struct Sample {
    name: String,
    rate: u64,
    key_field: Option<String>,
    exclude: Option<Condition>,
    count: u64,
}

impl Sample {
    // This function is dead code when the feature flag `transforms-impl-sample` is specified but not
    // `transforms-sample`.
    #![allow(dead_code)]
    pub const fn new(
        name: String,
        rate: u64,
        key_field: Option<String>,
        exclude: Option<Condition>,
    ) -> Self {
        Self {
            name,
            rate,
            key_field,
            exclude,
            count: 0,
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

        let num = if let Some(value) = value {
            seahash::hash(value.as_bytes())
        } else {
            self.count
        };

        self.count = (self.count + 1) % self.rate;

        if num % self.rate == 0 {
            match event {
                Event::Log(ref mut event) => {
                    event.namespace().insert_source_metadata(
                        self.name.as_str(),
                        event,
                        Some(LegacyKey::Overwrite(vrl::path!("sample_rate"))),
                        vrl::path!("sample_rate"),
                        self.rate.to_string(),
                    );
                }
                Event::Trace(ref mut event) => {
                    event.insert(event_path!("sample_rate"), self.rate.to_string());
                }
                Event::Metric(_) => panic!("component can never receive metric events"),
            };
            output.push(event);
        } else {
            emit!(SampleEventDiscarded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        conditions::{Condition, ConditionalConfig, VrlConfig},
        config::log_schema,
        event::{Event, LogEvent, TraceEvent},
        test_util::random_lines,
        transforms::test::transform_one,
        transforms::OutputBuffer,
    };
    use approx::assert_relative_eq;

    fn condition_contains(key: &str, needle: &str) -> Condition {
        let vrl_config = VrlConfig {
            source: format!(r#"contains!(."{}", "{}")"#, key, needle),
            runtime: Default::default(),
        };

        vrl_config
            .build(&Default::default())
            .expect("should not fail to build VRL condition")
    }

    #[test]
    fn hash_samples_at_roughly_the_configured_rate() {
        let num_events = 10000;

        let events = random_events(num_events);
        let mut sampler = Sample::new(
            "sample".to_string(),
            2,
            log_schema().message_key().map(ToString::to_string),
            Some(condition_contains(
                log_schema().message_key().unwrap().to_string().as_str(),
                "na",
            )),
        );
        let total_passed = events
            .into_iter()
            .filter_map(|event| {
                let mut buf = OutputBuffer::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.into_events().next()
            })
            .count();
        let ideal = 1.0f64 / 2.0f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);

        let events = random_events(num_events);
        let mut sampler = Sample::new(
            "sample".to_string(),
            25,
            log_schema().message_key().map(ToString::to_string),
            Some(condition_contains(
                log_schema().message_key().unwrap().to_string().as_str(),
                "na",
            )),
        );
        let total_passed = events
            .into_iter()
            .filter_map(|event| {
                let mut buf = OutputBuffer::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.into_events().next()
            })
            .count();
        let ideal = 1.0f64 / 25.0f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);
    }

    #[test]
    fn hash_consistently_samples_the_same_events() {
        let events = random_events(1000);
        let mut sampler = Sample::new(
            "sample".to_string(),
            2,
            log_schema().message_key().map(ToString::to_string),
            Some(condition_contains(
                log_schema().message_key().unwrap().to_string().as_str(),
                "na",
            )),
        );

        let first_run = events
            .clone()
            .into_iter()
            .filter_map(|event| {
                let mut buf = OutputBuffer::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.into_events().next()
            })
            .collect::<Vec<_>>();
        let second_run = events
            .into_iter()
            .filter_map(|event| {
                let mut buf = OutputBuffer::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.into_events().next()
            })
            .collect::<Vec<_>>();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn always_passes_events_matching_pass_list() {
        for key_field in &[None, log_schema().message_key().map(ToString::to_string)] {
            let event = Event::Log(LogEvent::from("i am important"));
            let mut sampler = Sample::new(
                "sample".to_string(),
                0,
                key_field.clone(),
                Some(condition_contains(
                    log_schema().message_key().unwrap().to_string().as_str(),
                    "important",
                )),
            );
            let iterations = 0..1000;
            let total_passed = iterations
                .filter_map(|_| {
                    transform_one(&mut sampler, event.clone())
                        .map(|result| assert_eq!(result, event))
                })
                .count();
            assert_eq!(total_passed, 1000);
        }
    }

    #[test]
    fn handles_key_field() {
        for key_field in &[None, Some("other_field".into())] {
            let mut event = Event::Log(LogEvent::from("nananana"));
            let log = event.as_mut_log();
            log.insert("other_field", "foo");
            let mut sampler = Sample::new(
                "sample".to_string(),
                0,
                key_field.clone(),
                Some(condition_contains("other_field", "foo")),
            );
            let iterations = 0..1000;
            let total_passed = iterations
                .filter_map(|_| {
                    transform_one(&mut sampler, event.clone())
                        .map(|result| assert_eq!(result, event))
                })
                .count();
            assert_eq!(total_passed, 1000);
        }
    }

    #[test]
    fn sampler_adds_sampling_rate_to_event() {
        for key_field in &[None, log_schema().message_key().map(ToString::to_string)] {
            let events = random_events(10000);
            let message_key = log_schema().message_key().unwrap().to_string();
            let mut sampler = Sample::new(
                "sample".to_string(),
                10,
                key_field.clone(),
                Some(condition_contains(&message_key, "na")),
            );
            let passing = events
                .into_iter()
                .filter(|s| !s.as_log()[&message_key].to_string_lossy().contains("na"))
                .find_map(|event| transform_one(&mut sampler, event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "10".into());

            let events = random_events(10000);
            let mut sampler = Sample::new(
                "sample".to_string(),
                25,
                key_field.clone(),
                Some(condition_contains(&message_key, "na")),
            );
            let passing = events
                .into_iter()
                .filter(|s| !s.as_log()[&message_key].to_string_lossy().contains("na"))
                .find_map(|event| transform_one(&mut sampler, event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "25".into());

            // If the event passed the regex check, don't include the sampling rate
            let mut sampler = Sample::new(
                "sample".to_string(),
                25,
                key_field.clone(),
                Some(condition_contains(&message_key, "na")),
            );
            let event = Event::Log(LogEvent::from("nananana"));
            let passing = transform_one(&mut sampler, event).unwrap();
            assert!(passing.as_log().get("sample_rate").is_none());
        }
    }

    #[test]
    fn handles_trace_event() {
        let event: TraceEvent = LogEvent::from("trace").into();
        let trace = Event::Trace(event);
        let mut sampler = Sample::new("sample".to_string(), 2, None, None);
        let iterations = 0..2;
        let total_passed = iterations
            .filter_map(|_| transform_one(&mut sampler, trace.clone()))
            .count();
        assert_eq!(total_passed, 1);
    }

    fn random_events(n: usize) -> Vec<Event> {
        random_lines(10)
            .take(n)
            .map(|e| Event::Log(LogEvent::from(e)))
            .collect()
    }
}
