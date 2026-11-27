use std::{hash::Hash, num::NonZeroU32, pin::Pin, time::Duration};

use async_stream::stream;
use futures::{Stream, StreamExt};
use governor::{Quota, clock};
use snafu::Snafu;
use vector_lib::{internal_event::emit, lookup::lookup_v2::ConfigTargetPath};

use super::{
    config::{ThrottleConfig, ThrottleInternalMetricsConfig},
    rate_limiter::RateLimiterRunner,
};
use crate::{
    conditions::Condition,
    config::TransformContext,
    event::{Event, Value},
    internal_events::{TemplateRenderingError, ThrottleEventDiscarded},
    template::UnconfinedTemplate,
    transforms::TaskTransform,
};

#[derive(Clone)]
pub struct Throttle<C: clock::Clock<Instant = I>, I: clock::Reference> {
    threshold: NonZeroU32,
    static_quota: Option<Quota>,
    window: Duration,
    key_field: Option<UnconfinedTemplate>,
    threshold_field: Option<ConfigTargetPath>,
    exclude: Option<Condition>,
    pub clock: C,
    internal_metrics: ThrottleInternalMetricsConfig,
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference,
{
    /// # Errors
    ///
    /// Returns an error if the rate limit is invalid or the exclusion condition cannot be built.
    pub fn new(
        config: &ThrottleConfig,
        context: &TransformContext,
        clock: C,
    ) -> crate::Result<Self> {
        let threshold = NonZeroU32::new(config.threshold).ok_or(ConfigError::NonZero)?;
        let window = config.window_secs;
        if window.is_zero() {
            return Err(Box::new(ConfigError::NonZero));
        }
        let static_quota = if config.threshold_field.is_none() {
            let period = Duration::from_secs_f64(window.as_secs_f64() / f64::from(threshold.get()));
            Some(
                Quota::with_period(period)
                    .ok_or(ConfigError::ZeroPeriod)?
                    .allow_burst(threshold),
            )
        } else {
            None
        };
        let exclude = config
            .exclude
            .as_ref()
            .map(|condition| condition.build(&context.enrichment_tables, &context.metrics_storage))
            .transpose()?;

        Ok(Self {
            threshold,
            static_quota,
            clock,
            window,
            key_field: config.key_field.clone(),
            threshold_field: config.threshold_field.clone(),
            exclude,
            internal_metrics: config.internal_metrics.clone(),
        })
    }

    #[must_use]
    pub fn start_rate_limiter<K>(&self) -> RateLimiterRunner<K, C>
    where
        K: Hash + Eq + Clone + Send + Sync + 'static,
    {
        RateLimiterRunner::start(self.static_quota, self.window, self.clock.clone())
    }

    fn event_threshold(&self, event: &Event) -> Option<NonZeroU32> {
        let path = self.threshold_field.as_ref()?;
        match event.as_log().get(path)? {
            Value::Integer(value) => u32::try_from(*value).ok().and_then(NonZeroU32::new),
            Value::String(value) => value.parse::<NonZeroU32>().ok(),
            Value::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.parse::<NonZeroU32>().ok(),
            _ => None,
        }
    }

    pub fn emit_event_discarded(&self, key: String) {
        emit(ThrottleEventDiscarded {
            key,
            emit_events_discarded_per_key: self.internal_metrics.emit_events_discarded_per_key,
            include_group_tag: self.internal_metrics.include_group_tag,
        });
    }
}

impl<C, I> TaskTransform<Event> for Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let limiter = self.start_rate_limiter();

        Box::pin(stream! {
            while let Some(event) = input_rx.next().await {
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
                                emit(TemplateRenderingError {
                                    error,
                                    field: Some("key_field"),
                                    drop_event: false,
                                });
                            })
                            .ok()
                    });

                    let threshold = self.event_threshold(&event).unwrap_or(self.threshold);
                    if limiter.check_key(&key, threshold) {
                        Some(event)
                    } else {
                        self.emit_event_discarded(key.unwrap_or_else(|| "None".to_string()));
                        None
                    }
                } else {
                    Some(event)
                };
                if let Some(event) = output {
                    yield event;
                }
            }
        })
    }
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("`threshold` and `window_secs` must be non-zero"))]
    NonZero,
    #[snafu(display("`window_secs` divided by `threshold` must round to at least one nanosecond"))]
    ZeroPeriod,
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use futures::SinkExt;
    use indoc::indoc;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vrl::{event_path, metadata_path};

    use super::*;
    use crate::{
        event::LogEvent,
        test_util::components::assert_transform_compliance,
        transforms::{Transform, test::create_topology},
    };

    async fn throttle_logs(config: &ThrottleConfig, logs: Vec<LogEvent>) -> Vec<Event> {
        let throttle = Throttle::new(
            config,
            &TransformContext::default(),
            clock::FakeRelativeClock::default(),
        )
        .unwrap();

        Box::new(throttle)
            .transform(Box::pin(futures::stream::iter(
                logs.into_iter().map(Event::Log),
            )))
            .collect()
            .await
    }

    #[tokio::test]
    async fn threshold_field_overrides_static_threshold_or_falls_back() {
        let config: ThrottleConfig = serde_yaml::from_str(indoc! {"
            threshold: 2
            threshold_field: limit
            window_secs: 1
        "})
        .unwrap();

        for (value, expected) in [
            (Some(Value::Integer(1)), 1),
            (Some(Value::Bytes("3".into())), 3),
            (Some(Value::String("3".into())), 3),
            (Some(Value::Integer(i64::from(u32::MAX))), 4),
            (None, 2),
            (Some(Value::Integer(0)), 2),
            (Some(Value::Integer(-1)), 2),
            (Some(Value::Integer(i64::from(u32::MAX) + 1)), 2),
            (Some(Value::from(3.0)), 2),
            (Some(Value::String("invalid".into())), 2),
            (Some(Value::Bytes(vec![0xff].into())), 2),
        ] {
            let mut log = LogEvent::from("hello");
            if let Some(value) = value.clone() {
                log.insert(event_path!("limit"), value);
            }
            let output = throttle_logs(&config, vec![log.clone(); 4]).await;
            assert_eq!(output, vec![Event::Log(log); expected], "value: {value:?}");
        }
    }

    #[test]
    fn rejects_template_as_threshold_field_path() {
        let config = indoc! {r#"
            threshold: 1
            threshold_field: "{{ threshold }}"
            window_secs: 1
        "#};
        let error = serde_yaml::from_str::<ThrottleConfig>(config).unwrap_err();
        assert!(error.to_string().contains("{{ threshold }}"));
    }

    #[tokio::test]
    async fn threshold_field_supports_event_and_metadata_paths() {
        let mut log = LogEvent::from("hello");
        log.insert(event_path!("limits", "rate"), 2);
        log.insert(metadata_path!("limits", "rate"), 3);

        for (path, expected) in [(".limits.rate", 2), ("%limits.rate", 3)] {
            let config = ThrottleConfig {
                threshold: 1,
                threshold_field: Some(path.into()),
                window_secs: Duration::from_secs(5),
                ..Default::default()
            };
            assert_eq!(
                throttle_logs(&config, vec![log.clone(); 4]).await.len(),
                expected,
                "path: {path}",
            );
        }
    }

    #[tokio::test]
    async fn threshold_field_shares_usage_when_threshold_changes() {
        let config = ThrottleConfig {
            threshold: 2,
            threshold_field: Some("limit".into()),
            window_secs: Duration::from_secs(8),
            ..Default::default()
        };

        for thresholds in [[2, 4, 4, 4], [4, 4, 2, 4], [4, 4, 0, 4]] {
            let logs: Vec<_> = thresholds
                .into_iter()
                .map(|threshold| {
                    let mut log = LogEvent::from("hello");
                    log.insert(event_path!("limit"), threshold);
                    log
                })
                .collect();
            let expected: Vec<_> = logs[..3].iter().cloned().map(Event::Log).collect();

            assert_eq!(throttle_logs(&config, logs).await, expected);
        }
    }

    #[tokio::test]
    async fn threshold_field_rate_limits_buckets_independently() {
        let config: ThrottleConfig = serde_yaml::from_str(indoc! {r#"
            threshold: 2
            threshold_field: limit
            window_secs: 5
            key_field: "{{ service }}"
        "#})
        .unwrap();
        let logs = [("a", 1), ("b", 1), ("c", 3)]
            .into_iter()
            .cycle()
            .take(12)
            .map(|(service, limit)| {
                let mut log = LogEvent::from(service);
                log.insert(event_path!("service"), service);
                log.insert(event_path!("limit"), limit);
                log
            })
            .collect();

        let services: Vec<_> = throttle_logs(&config, logs)
            .await
            .into_iter()
            .map(|event| event.as_log()["service"].clone())
            .collect();
        assert_eq!(services, ["a", "b", "c", "c", "c"].map(Value::from));
    }

    #[tokio::test]
    async fn throttle_events() {
        let clock = clock::FakeRelativeClock::default();
        let config = serde_yaml::from_str::<ThrottleConfig>(indoc! {"
            threshold: 2
            window_secs: 5
        "})
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
            match out_stream.next().await {
                Some(_event) => {
                    count += 1;
                }
                _ => {
                    panic!("Unexpectedly received None in output stream");
                }
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
        match out_stream.next().await {
            Some(_event) => {}
            _ => {
                panic!("Unexpectedly received None in output stream");
            }
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
        let config = serde_yaml::from_str::<ThrottleConfig>(indoc! {"
            threshold: 2
            window_secs: 5
            exclude: \"exists(.special)\"
        "})
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
            match out_stream.next().await {
                Some(_event) => {
                    count += 1;
                }
                _ => {
                    panic!("Unexpectedly received None in output stream");
                }
            }
        }
        assert_eq!(2, count);

        clock.advance(Duration::from_secs(2));

        tx.send(LogEvent::default().into()).await.unwrap();

        // We should be back to pending, having the second event dropped
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        let mut special_log = LogEvent::default();
        special_log.insert(event_path!("special"), "true");
        tx.send(special_log.into()).await.unwrap();
        // The rate limiter should allow this log through regardless of current limit
        match out_stream.next().await {
            Some(_event) => {}
            _ => {
                panic!("Unexpectedly received None in output stream");
            }
        }

        clock.advance(Duration::from_secs(3));

        tx.send(LogEvent::default().into()).await.unwrap();

        // The rate limiter should now be refreshed and allow an additional event through
        match out_stream.next().await {
            Some(_event) => {}
            _ => {
                panic!("Unexpectedly received None in output stream");
            }
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
        let config = serde_yaml::from_str::<ThrottleConfig>(indoc! {r#"
            threshold: 1
            window_secs: 5
            key_field: "{{ bucket }}"
        "#})
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
        log_a.insert(event_path!("bucket"), "a");
        let mut log_b = LogEvent::default();
        log_b.insert(event_path!("bucket"), "b");
        tx.send(log_a.into()).await.unwrap();
        tx.send(log_b.into()).await.unwrap();

        let mut count = 0_u8;
        while count < 2 {
            match out_stream.next().await {
                Some(_event) => {
                    count += 1;
                }
                _ => {
                    panic!("Unexpectedly received None in output stream");
                }
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
        Box::pin(assert_transform_compliance(async move {
            let config = ThrottleConfig {
                threshold: 1,
                threshold_field: None,
                window_secs: Duration::from_secs_f64(1.0),
                key_field: None,
                exclude: None,
                internal_metrics: ThrottleInternalMetricsConfig::default(),
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let log = LogEvent::from("hello world");
            tx.send(log.into()).await.unwrap();

            _ = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        }))
        .await;
    }
}
