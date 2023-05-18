use std::{num::NonZeroU32, time::Duration};

use dashmap::DashMap;
use governor::{clock, state::InMemoryState, Quota, RateLimiter, middleware::NoOpMiddleware};
use serde_with::serde_as;
use snafu::Snafu;
use vector_config::configurable_component;
use vector_core::config::{clone_input_definitions, LogNamespace};

use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    event::Event,
    internal_events::{TemplateRenderingError, ThrottleEventDiscarded},
    schema,
    template::Template,
    transforms::{TickTransform, Transform, TransformOutputsBuf},
};

/// Configuration for the `throttle` transform.
#[serde_as]
#[configurable_component(transform("throttle", "Rate limit logs passing through a topology."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleConfig {
    /// The number of events allowed for a given bucket per configured `window_secs`.
    ///
    /// Each unique key has its own `threshold`.
    threshold: u32,

    /// The time window in which the configured `threshold` is applied, in seconds.
    #[serde_as(as = "serde_with::DurationSeconds<f64>")]
    window_secs: Duration,

    /// The value to group events into separate buckets to be rate limited independently.
    ///
    /// If left unspecified, or if the event doesn't have `key_field`, then the event is not rate
    /// limited separately.
    #[configurable(metadata(docs::examples = "{{ message }}", docs::examples = "{{ hostname }}",))]
    key_field: Option<Template>,

    /// A logical condition used to exclude events from sampling.
    exclude: Option<AnyCondition>,
}

impl_generate_config_from_default!(ThrottleConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "throttle")]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        let throttle = Throttle::new(self, context, clock::MonotonicClock)?;
        Ok(Transform::tick(throttle, self.window_secs * 2))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        // The event is not modified, so the definition is passed through as-is
        vec![TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        )]
    }
}

pub struct Throttle<C: clock::Clock<Instant = I>, I: clock::Reference> {
    key_field: Option<Template>,
    limiter: RateLimiter<Option<String>, DashMap<Option<String>, InMemoryState>, C, NoOpMiddleware<I>>,
    exclude: Option<Condition>,
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I>,
    I: clock::Reference,
{
    pub fn new(
        config: &ThrottleConfig,
        context: &TransformContext,
        clock: C,
    ) -> crate::Result<Self> {
        let flush_keys_interval = config.window_secs;

        let threshold = match NonZeroU32::new(config.threshold) {
            Some(threshold) => threshold,
            None => return Err(Box::new(ConfigError::NonZero)),
        };

        let quota = match Quota::with_period(Duration::from_secs_f64(
            flush_keys_interval.as_secs_f64() / f64::from(threshold.get()),
        )) {
            Some(quota) => quota.allow_burst(threshold),
            None => return Err(Box::new(ConfigError::NonZero)),
        };

        let limiter = RateLimiter::dashmap_with_clock(quota, &clock);

        let exclude = config
            .exclude
            .as_ref()
            .map(|condition| condition.build(&context.enrichment_tables))
            .transpose()?;

        Ok(Self {
            limiter,
            key_field: config.key_field.clone(),
            exclude,
        })
    }
}

impl<C, I> TickTransform for Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Send + 'static,
    I: clock::Reference + Send + 'static,
{
    fn tick(&mut self, _output: &mut TransformOutputsBuf) {
        self.limiter.retain_recent();
    }

    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        let (throttle, event) = match self.exclude.as_ref() {
            Some(condition) => {
                let (result, event) = condition.check(event);
                (!result, event)
            }
            _ => (true, event),
        };
        let value = if throttle {
            let key = self.key_field.as_ref().and_then(|t| {
                t.render_string(&event)
                    .map_err(|error| {
                        emit!(TemplateRenderingError {
                            error,
                            field: Some("key_field"),
                            drop_event: false,
                        })
                    })
                    .ok()
            });

            match self.limiter.check_key(&key) {
                Ok(()) => Some(event),
                _ => {
                    if let Some(key) = key {
                        emit!(ThrottleEventDiscarded { key })
                    } else {
                        emit!(ThrottleEventDiscarded {
                            key: "None".to_string()
                        })
                    }
                    None
                }
            }
        } else {
            Some(event)
        };
        if let Some(event) = value {
            output.push(event);
        }
    }

    fn finish(&mut self, _output: &mut TransformOutputsBuf) {}
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("`threshold`, and `window_secs` must be non-zero"))]
    NonZero,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::LogEvent, test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ThrottleConfig>();
    }

    #[tokio::test]
    async fn throttle_events() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 2
window_secs = 5
"#,
        )
        .unwrap();

        let mut throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()).unwrap();

        let mut out = TransformOutputsBuf::new_with_primary();
        throttle.transform(LogEvent::default().into(), &mut out);
        throttle.transform(LogEvent::default().into(), &mut out);

        assert_eq!(2, out.len());

        clock.advance(Duration::from_secs(2));

        throttle.transform(LogEvent::default().into(), &mut out);

        // Still only two
        assert_eq!(2, out.len());

        clock.advance(Duration::from_secs(3));

        throttle.transform(LogEvent::default().into(), &mut out);

        // The rate limiter should now be refreshed and allow an additional event through
        assert_eq!(3, out.len());

        throttle.finish(&mut out);

        // And still three
        assert_eq!(3, out.len());
    }

    #[tokio::test]
    async fn throttle_exclude() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 2
window_secs = 5
exclude = """
exists(.special)
"""
"#,
        )
        .unwrap();

        let mut throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()) .unwrap();

        let mut out = TransformOutputsBuf::new_with_primary();

        throttle.transform(LogEvent::default().into(), &mut out);
        throttle.transform(LogEvent::default().into(), &mut out);

        assert_eq!(2, out.len());

        clock.advance(Duration::from_secs(2));

        throttle.transform(LogEvent::default().into(), &mut out);

        // Still only two
        assert_eq!(2, out.len());

        let mut special_log = LogEvent::default();
        special_log.insert("special", "true");

        throttle.transform(special_log.into(), &mut out);
        // The rate limiter should allow this log through regardless of current limit
        assert_eq!(3, out.len());

        clock.advance(Duration::from_secs(3));

        throttle.transform(LogEvent::default().into(), &mut out);

        // The rate limiter should now be refreshed and allow an additional event through
        assert_eq!(4, out.len());

        throttle.finish(&mut out);

        // And nothing more
        assert_eq!(4, out.len());
    }

    #[tokio::test]
    async fn throttle_buckets() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 1
window_secs = 5
key_field = "{{ bucket }}"
"#,
        )
        .unwrap();

        let mut throttle = Throttle::new(&config, &TransformContext::default(), clock.clone()) .unwrap();

        let mut out = TransformOutputsBuf::new_with_primary();

        let mut log_a = LogEvent::default();
        log_a.insert("bucket", "a");
        let mut log_b = LogEvent::default();
        log_b.insert("bucket", "b");
        throttle.transform(log_a.into(), &mut out);
        throttle.transform(log_b.into(), &mut out);

        assert_eq!(2, out.len());

        throttle.finish(&mut out);

        // And nothing more
        assert_eq!(2, out.len());
    }

    #[tokio::test]
    async fn emits_internal_events() {
        assert_transform_compliance(async move {
            let config = ThrottleConfig {
                threshold: 1,
                window_secs: Duration::from_secs_f64(1.0),
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
}
