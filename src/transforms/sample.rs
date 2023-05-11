use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::Event,
    internal_events::SampleEventDiscarded,
    schema,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `sample` transform.
#[configurable_component(transform(
    "sample",
    "Sample events from an event stream based on supplied criteria and at a configurable rate."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SampleConfig {
    /// The rate at which events are forwarded, expressed as `1/N`.
    ///
    /// For example, `rate = 10` means 1 out of every 10 events are forwarded and the rest are
    /// dropped.
    pub rate: u64,

    /// The name of the field whose value is hashed to determine if the event should be
    /// sampled.
    ///
    /// Each unique value for the key creates a bucket of related events to be sampled together
    /// and the rate is applied to the buckets themselves to sample `1/N` buckets.  The overall rate
    /// of sampling may differ from the configured one if values in the field are not uniformly
    /// distributed. If left unspecified, or if the event doesn’t have `key_field`, then the
    /// event is sampled independently.
    ///
    /// This can be useful to, for example, ensure that all logs for a given transaction are
    /// sampled together, but that overall `1/N` transactions are sampled.
    #[configurable(metadata(docs::examples = "message",))]
    pub key_field: Option<String>,

    /// A logical condition used to exclude events from sampling.
    pub exclude: Option<AnyCondition>,
}

impl GenerateConfig for SampleConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            rate: 10,
            key_field: None,
            exclude: None::<AnyCondition>,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sample")]
impl TransformConfig for SampleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Sample::new(
            self.rate,
            self.key_field.clone(),
            self.exclude
                .as_ref()
                .map(|condition| condition.build(&context.enrichment_tables))
                .transpose()?,
        )))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Trace)
    }

    fn outputs(
        &self,
        _: enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::Log | DataType::Trace,
            input_definitions
                .iter()
                .map(|(output, definition)| (output.clone(), definition.clone()))
                .collect(),
        )]
    }
}

#[derive(Clone)]
pub struct Sample {
    rate: u64,
    key_field: Option<String>,
    exclude: Option<Condition>,
    count: u64,
}

impl Sample {
    pub const fn new(rate: u64, key_field: Option<String>, exclude: Option<Condition>) -> Self {
        Self {
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
                Event::Log(event) => event.get(key_field.as_str()),
                Event::Trace(event) => event.get(key_field.as_str()),
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
                Event::Log(ref mut event) => event.insert("sample_rate", self.rate.to_string()),
                Event::Trace(ref mut event) => event.insert("sample_rate", self.rate.to_string()),
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
    use approx::assert_relative_eq;

    use super::*;
    use crate::{
        conditions::{Condition, ConditionalConfig, VrlConfig},
        config::log_schema,
        event::{Event, LogEvent, TraceEvent},
        test_util::{components::assert_transform_compliance, random_lines},
        transforms::test::{create_topology, transform_one},
    };
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

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
    fn generate_config() {
        crate::test_util::test_generate_config::<SampleConfig>();
    }

    #[test]
    fn hash_samples_at_roughly_the_configured_rate() {
        let num_events = 10000;

        let events = random_events(num_events);
        let mut sampler = Sample::new(
            2,
            Some(log_schema().message_key().into()),
            Some(condition_contains(log_schema().message_key(), "na")),
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
            25,
            Some(log_schema().message_key().into()),
            Some(condition_contains(log_schema().message_key(), "na")),
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
            2,
            Some(log_schema().message_key().into()),
            Some(condition_contains(log_schema().message_key(), "na")),
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
        for key_field in &[None, Some(log_schema().message_key().into())] {
            let event = Event::Log(LogEvent::from("i am important"));
            let mut sampler = Sample::new(
                0,
                key_field.clone(),
                Some(condition_contains(log_schema().message_key(), "important")),
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
        for key_field in &[None, Some(log_schema().message_key().into())] {
            let events = random_events(10000);
            let mut sampler = Sample::new(
                10,
                key_field.clone(),
                Some(condition_contains(log_schema().message_key(), "na")),
            );
            let passing = events
                .into_iter()
                .filter(|s| {
                    !s.as_log()[log_schema().message_key()]
                        .to_string_lossy()
                        .contains("na")
                })
                .find_map(|event| transform_one(&mut sampler, event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "10".into());

            let events = random_events(10000);
            let mut sampler = Sample::new(
                25,
                key_field.clone(),
                Some(condition_contains(log_schema().message_key(), "na")),
            );
            let passing = events
                .into_iter()
                .filter(|s| {
                    !s.as_log()[log_schema().message_key()]
                        .to_string_lossy()
                        .contains("na")
                })
                .find_map(|event| transform_one(&mut sampler, event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "25".into());

            // If the event passed the regex check, don't include the sampling rate
            let mut sampler = Sample::new(
                25,
                key_field.clone(),
                Some(condition_contains(log_schema().message_key(), "na")),
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
        let mut sampler = Sample::new(2, None, None);
        let iterations = 0..2;
        let total_passed = iterations
            .filter_map(|_| transform_one(&mut sampler, trace.clone()))
            .count();
        assert_eq!(total_passed, 1);
    }

    #[tokio::test]
    async fn emits_internal_events() {
        assert_transform_compliance(async move {
            let config = SampleConfig {
                rate: 1,
                key_field: None,
                exclude: None,
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let log = LogEvent::from("hello world");
            tx.send(log.into()).await.unwrap();

            _ = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await
    }

    fn random_events(n: usize) -> Vec<Event> {
        random_lines(10)
            .take(n)
            .map(|e| Event::Log(LogEvent::from(e)))
            .collect()
    }
}
