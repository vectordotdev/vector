use std::{num::NonZeroU32, pin::Pin, time::Duration};

use async_stream::stream;
use futures::{Stream, StreamExt};
use governor::{clock, Quota, RateLimiter};
use serde_with::serde_as;
use snafu::Snafu;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, Input, Output, TransformConfig, TransformContext},
    event::Event,
    internal_events::{TemplateRenderingError, ThrottleEventDiscarded},
    schema,
    template::Template,
    transforms::{TaskTransform, Transform},
};

/// Configuration for the `throttle` transform.
#[serde_as]
#[configurable_component(transform("throttle"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ThrottleConfig {
    /// The number of events allowed for a given bucket per configured `window_secs`.
    ///
    /// Each unique key will have its own `threshold`.
    threshold: u32,

    /// The time window in which the configured `threshold` is applied, in seconds.
    #[serde_as(as = "serde_with::DurationSeconds<f64>")]
    window_secs: Duration,

    /// The name of the log field whose value will be hashed to determine if the event should be
    /// rate limited.
    ///
    /// Each unique key will create a bucket of related events to be rate limited separately. If
    /// left unspecified, or if the event doesn’t have `key_field`, the event be will not be rate
    /// limited separately.
    #[configurable(metadata(docs::examples = "{{ message }}", docs::examples = "{{ hostname }}",))]
    key_field: Option<Template>,

    /// A logical condition used to exclude events from sampling.
    exclude: Option<AnyCondition>,
}

impl_generate_config_from_default!(ThrottleConfig);

#[async_trait::async_trait]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Throttle::new(self, context, clock::MonotonicClock).map(Transform::event_task)
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(&self, merged_definition: &schema::Definition, _: LogNamespace) -> Vec<Output> {
        // The event is not modified, so the definition is passed through as-is
        vec![Output::default(DataType::Log).with_schema_definition(merged_definition.clone())]
    }
}

#[derive(Clone)]
pub struct Throttle<C: clock::Clock<Instant = I>, I: clock::Reference> {
    quota: Quota,
    flush_keys_interval: Duration,
    key_field: Option<Template>,
    exclude: Option<Condition>,
    clock: C,
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
        let exclude = config
            .exclude
            .as_ref()
            .map(|condition| condition.build(&context.enrichment_tables))
            .transpose()?;

        Ok(Self {
            quota,
            clock,
            flush_keys_interval,
            key_field: config.key_field.clone(),
            exclude,
        })
    }
}

impl<C, I> TaskTransform<Event> for Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Send + 'static,
    I: clock::Reference + Send + 'static,
{
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut flush_keys = tokio::time::interval(self.flush_keys_interval * 2);

        let limiter = RateLimiter::dashmap_with_clock(self.quota, &self.clock);

        Box::pin(stream! {
          loop {
            let done = tokio::select! {
                biased;

                maybe_event = input_rx.next() => {
                    match maybe_event {
                        None => true,
                        Some(event) => {
                            let (throttle, event) = match self.exclude.as_ref() {
                                Some(condition) => {
                                    let (result, event) = condition.check(event);
                                    (!result, event)
                                },
                                _ => (true, event)
                            };
                            let output = if throttle {
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

                                match limiter.check_key(&key) {
                                    Ok(()) => {
                                        Some(event)
                                    }
                                    _ => {
                                        if let Some(key) = key {
                                            emit!(ThrottleEventDiscarded{key})
                                        } else {
                                            emit!(ThrottleEventDiscarded{key: "None".to_string()})
                                        }
                                        None
                                    }
                                }
                            } else {
                                Some(event)
                            };
                            if let Some(event) = output {
                                yield event;
                            }
                            false
                        }
                    }
                }
                _ = flush_keys.tick() => {
                    limiter.retain_recent();
                    false
                }
            };
            if done { break }
          }
        })
    }
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("`threshold`, and `window_secs` must be non-zero"))]
    NonZero,
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use futures::SinkExt;

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

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone())
            .map(Transform::event_task)
            .unwrap();

        let throttle = throttle.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = throttle.transform_events(Box::pin(rx));

        // tokio interval is always immediately ready, so we poll once to make sure
        // we trip it/set the interval in the future
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.send(LogEvent::default().into()).await.unwrap();
        tx.send(LogEvent::default().into()).await.unwrap();

        let mut count = 0_u8;
        while count < 2 {
            if let Some(_event) = out_stream.next().await {
                count += 1;
            } else {
                panic!("Unexpectedly received None in output stream");
            }
        }
        assert_eq!(2, count);

        clock.advance(Duration::from_secs(2));

        tx.send(LogEvent::default().into()).await.unwrap();

        // We should be back to pending, having the second event dropped
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        clock.advance(Duration::from_secs(3));

        tx.send(LogEvent::default().into()).await.unwrap();

        // The rate limiter should now be refreshed and allow an additional event through
        if let Some(_event) = out_stream.next().await {
        } else {
            panic!("Unexpectedly received None in output stream");
        }

        // We should be back to pending, having nothing waiting for us
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.disconnect();

        // And still nothing there
        assert_eq!(Poll::Ready(None), futures::poll!(out_stream.next()));
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

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone())
            .map(Transform::event_task)
            .unwrap();

        let throttle = throttle.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = throttle.transform_events(Box::pin(rx));

        // tokio interval is always immediately ready, so we poll once to make sure
        // we trip it/set the interval in the future
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.send(LogEvent::default().into()).await.unwrap();
        tx.send(LogEvent::default().into()).await.unwrap();

        let mut count = 0_u8;
        while count < 2 {
            if let Some(_event) = out_stream.next().await {
                count += 1;
            } else {
                panic!("Unexpectedly received None in output stream");
            }
        }
        assert_eq!(2, count);

        clock.advance(Duration::from_secs(2));

        tx.send(LogEvent::default().into()).await.unwrap();

        // We should be back to pending, having the second event dropped
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        let mut special_log = LogEvent::default();
        special_log.insert("special", "true");
        tx.send(special_log.into()).await.unwrap();
        // The rate limiter should allow this log through regardless of current limit
        if let Some(_event) = out_stream.next().await {
        } else {
            panic!("Unexpectedly received None in output stream");
        }

        clock.advance(Duration::from_secs(3));

        tx.send(LogEvent::default().into()).await.unwrap();

        // The rate limiter should now be refreshed and allow an additional event through
        if let Some(_event) = out_stream.next().await {
        } else {
            panic!("Unexpectedly received None in output stream");
        }

        // We should be back to pending, having nothing waiting for us
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.disconnect();

        // And still nothing there
        assert_eq!(Poll::Ready(None), futures::poll!(out_stream.next()));
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

        let throttle = Throttle::new(&config, &TransformContext::default(), clock.clone())
            .map(Transform::event_task)
            .unwrap();

        let throttle = throttle.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = throttle.transform_events(Box::pin(rx));

        // tokio interval is always immediately ready, so we poll once to make sure
        // we trip it/set the interval in the future
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        let mut log_a = LogEvent::default();
        log_a.insert("bucket", "a");
        let mut log_b = LogEvent::default();
        log_b.insert("bucket", "b");
        tx.send(log_a.into()).await.unwrap();
        tx.send(log_b.into()).await.unwrap();

        let mut count = 0_u8;
        while count < 2 {
            if let Some(_event) = out_stream.next().await {
                count += 1;
            } else {
                panic!("Unexpectedly received None in output stream");
            }
        }
        assert_eq!(2, count);

        // We should be back to pending, having nothing waiting for us
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.disconnect();

        // And still nothing there
        assert_eq!(Poll::Ready(None), futures::poll!(out_stream.next()));
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
