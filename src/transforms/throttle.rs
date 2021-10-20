use crate::conditions::{AnyCondition, Condition};
use crate::config::{DataType, TransformConfig, TransformContext, TransformDescription};
use crate::event::Event;
use crate::internal_events::TemplateRenderingFailed;
use crate::template::Template;
use crate::transforms::{TaskTransform, Transform};

use async_stream::stream;
use futures::{stream, Stream, StreamExt};
use governor::*;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::time::Duration;

#[derive(Deserialize, Default, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct ThrottleConfig {
    threshold: u32,
    window: f64,
    key_field: Option<Template>,
    exclude: Option<AnyCondition>,
}

inventory::submit! {
    TransformDescription::new::<ThrottleConfig>("throttle")
}

impl_generate_config_from_default!(ThrottleConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "throttle")]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Throttle::new(self, context, &clock::MonotonicClock).map(Transform::task)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "throttle"
    }
}

#[derive(Clone)]
pub struct Throttle<C: 'static + clock::Clock<Instant = I>, I: clock::Reference> {
    quota: Quota,
    flush_keys_interval: Duration,
    key_field: Option<Template>,
    exclude: Option<Box<dyn Condition>>,
    clock: &'static C,
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I>,
    I: clock::Reference,
{
    pub fn new(
        config: &ThrottleConfig,
        context: &TransformContext,
        clock: &'static C,
    ) -> crate::Result<Self> {
        let flush_keys_interval = Duration::from_secs_f64(config.window.clone());

        let threshold = match NonZeroU32::new(config.threshold) {
            Some(threshold) => threshold,
            None => return Err(Box::new(ConfigError::NonZero)),
        };

        let quota = match Quota::with_period(Duration::from_secs_f64(
            config.window / threshold.get() as f64,
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

impl<C, I> TaskTransform for Throttle<C, I>
where
    C: 'static + clock::Clock<Instant = I> + Send + Sync,
    I: clock::Reference + Send,
{
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut flush_keys = tokio::time::interval(self.flush_keys_interval * 2);

        let mut flush_stream = tokio::time::interval(Duration::from_millis(1000));

        let limiter = RateLimiter::dashmap_with_clock(self.quota, self.clock);

        Box::pin(
            stream! {
              loop {
                let mut output = Vec::new();
                let done = tokio::select! {
                    _ = flush_stream.tick() => {
                        false
                    }
                    _ = flush_keys.tick() => {
                        limiter.retain_recent();
                        false
                    }
                    maybe_event = input_rx.next() => {
                        match maybe_event {
                            None => true,
                            Some(event) => {
                                match self.exclude.as_ref() {
                                  Some(condition) if condition.check(&event) => output.push(event),
                                  _ => {
                                        let key = self.key_field.as_ref().and_then(|t| {
                                            t.render_string(&event)
                                                .map_err(|error| {
                                                    emit!(&TemplateRenderingFailed {
                                                        error,
                                                        field: Some("key_field"),
                                                        drop_event: false,
                                                    })
                                                })
                                                .ok()
                                        });

                                        match limiter.check_key(&key) {
                                            Ok(()) => {
                                                output.push(event);
                                            }
                                            _ => {
                                                // Dropping event
                                            }
                                        }
                                    }
                                }
                                false
                            }
                        }
                    }
                };
                yield stream::iter(output.into_iter());
                if done { break }
              }
            }
            .flatten(),
        )
    }
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("`threshold`, and `window` must be non-zero"))]
    NonZero,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use futures::SinkExt;
    use std::task::Poll;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ThrottleConfig>();
    }

    #[tokio::test]
    async fn throttle_events() {
        let clock = clock::FakeRelativeClock::default();
        let config = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 1
window = 5
"#,
        )
        .unwrap();

        let throttle = Throttle::new(&config, &TransformContext::default(), &clock)
            .map(Transform::task)
            .unwrap();

        let throttle = throttle.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = throttle.transform(Box::pin(rx));

        // tokio interval is always immediately ready, so we poll once to make sure
        // we trip it/set the interval in the future
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.send(Event::new_empty_log()).await.unwrap();

        if let Some(_event) = out_stream.next().await {
        } else {
            panic!("Unexpectedly recieved None in output stream");
        }

        clock.advance(Duration::from_secs(2));

        tx.send(Event::new_empty_log()).await.unwrap();

        // We should be back to pending, having the second event dropped
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        tx.send(Event::new_empty_log()).await.unwrap();

        // We should be back to pending, having nothing waiting for us
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));
        // Close the input stream which should trigger the shutting down flush
        tx.disconnect();

        // And still nothing there
        assert_eq!(Poll::Ready(None), futures::poll!(out_stream.next()));

        drop(out_stream);
    }

    #[tokio::test]
    async fn dont_throttle_events() {
        let throttle = toml::from_str::<ThrottleConfig>(
            r#"
threshold = 60
window = 60
"#,
        )
        .unwrap()
        .build(&TransformContext::default())
        .await
        .unwrap();

        let throttle = throttle.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = throttle.transform(Box::pin(rx));

        tokio::time::pause();

        // tokio interval is always immediately ready, so we poll once to make sure
        // we trip it/set the interval in the future
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        // Now send our events

        // We won't have flushed yet b/c the interval hasn't elapsed, so no outputs
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));
        // Now fast foward time enough that our flush should trigger.
        tokio::time::advance(Duration::from_secs(10)).await;

        // We should be back to pending, having nothing waiting for us
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));
        // Close the input stream which should trigger the shutting down flush
        tx.disconnect();

        // And still nothing there
        assert_eq!(Poll::Ready(None), futures::poll!(out_stream.next()));
    }
}
