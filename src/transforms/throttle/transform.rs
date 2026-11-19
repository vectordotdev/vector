use std::{
    hash::{Hash, Hasher},
    num::NonZeroU32,
    pin::Pin,
    time::Duration,
};
use vector_lib::internal_event::emit;

use futures::{Stream, StreamExt, future, stream::BoxStream};
use governor::{Quota, clock};
use snafu::Snafu;

use super::{
    config::{ThrottleConfig, ThrottleInternalMetricsConfig},
    rate_limiter::RateLimiterRunner,
};
use crate::{
    conditions::Condition,
    config::TransformContext,
    event::Event,
    internal_events::{TemplateRenderingError, ThrottleEventDiscarded},
    template::{TemplateRenderingError as RenderError, UnconfinedTemplate},
    transforms::{TaskTransform, partitioned::KeyedTransform},
};

/// The bucket an event is rate limited in, from its `key_field`.
///
/// It is computed without side effects, so that the router of
/// [`Partitioned`](crate::transforms::partitioned::Partitioned) can compute it. A rendering error
/// is kept and only reported once the event is known not to be excluded, as a single task does.
pub enum ThrottleKey {
    /// `key_field` is unset.
    Unkeyed,
    /// `key_field` rendered to this bucket.
    Bucket(String),
    /// `key_field` failed to render.
    RenderFailed(RenderError),
}

impl ThrottleKey {
    /// The bucket's name, or `None` for the shared bucket of events without a rendered key.
    fn bucket(&self) -> Option<&str> {
        match self {
            Self::Bucket(bucket) => Some(bucket),
            Self::Unkeyed | Self::RenderFailed(_) => None,
        }
    }
}

impl Hash for ThrottleKey {
    /// Only the bucket picks the shard: events whose key failed to render share the `None` bucket
    /// with unkeyed events, so they must reach the same shard.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bucket().hash(state);
    }
}

#[derive(Clone)]
pub struct Throttle<C: clock::Clock<Instant = I>, I: clock::Reference> {
    pub quota: Quota,
    pub flush_keys_interval: Duration,
    key_field: Option<UnconfinedTemplate>,
    exclude: Option<Condition>,
    pub clock: C,
    internal_metrics: ThrottleInternalMetricsConfig,
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference,
{
    ///
    /// # Errors
    ///
    /// Returns an error if the exclusion condition cannot be built.
    pub fn new(
        config: &ThrottleConfig,
        context: &TransformContext,
        clock: C,
    ) -> crate::Result<Self> {
        let flush_keys_interval = config.window_secs;

        let Some(threshold) = NonZeroU32::new(config.threshold) else {
            return Err(Box::new(ConfigError::NonZero));
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
            .map(|condition| condition.build(&context.enrichment_tables, &context.metrics_storage))
            .transpose()?;

        Ok(Self {
            quota,
            clock,
            flush_keys_interval,
            key_field: config.key_field.clone(),
            exclude,
            internal_metrics: config.internal_metrics.clone(),
        })
    }

    #[must_use]
    pub fn start_rate_limiter<K>(&self) -> RateLimiterRunner<K, C>
    where
        K: Hash + Eq + Clone + Send + Sync + 'static,
    {
        RateLimiterRunner::start(self)
    }

    pub fn emit_event_discarded(&self, key: String) {
        emit(ThrottleEventDiscarded {
            key,
            emit_events_discarded_per_key: self.internal_metrics.emit_events_discarded_per_key,
            include_group_tag: self.internal_metrics.include_group_tag,
        });
    }
}

impl<C, I> Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    /// Renders the bucket of `event`, without emitting anything.
    fn render_key(&self, event: &Event) -> ThrottleKey {
        let Some(template) = &self.key_field else {
            return ThrottleKey::Unkeyed;
        };
        template
            .render_string(event)
            .map_or_else(ThrottleKey::RenderFailed, ThrottleKey::Bucket)
    }

    /// Returns `event` if it is excluded or within its bucket's rate, and `None` if it is
    /// discarded. `key` computes the bucket, and is only called for events that aren't excluded.
    fn throttle_one(
        &self,
        limiter: &RateLimiterRunner<Option<String>, C>,
        event: Event,
        key: impl FnOnce(&Event) -> ThrottleKey,
    ) -> Option<Event> {
        let event = match self.exclude.as_ref() {
            Some(condition) => {
                let (excluded, event) = condition.check(event);
                if excluded {
                    return Some(event);
                }
                event
            }
            None => event,
        };

        let bucket = match key(&event) {
            ThrottleKey::Bucket(bucket) => Some(bucket),
            ThrottleKey::Unkeyed => None,
            ThrottleKey::RenderFailed(error) => {
                emit(TemplateRenderingError {
                    error,
                    field: Some("key_field"),
                    drop_event: false,
                });
                None
            }
        };

        if limiter.check_key(&bucket) {
            Some(event)
        } else {
            self.emit_event_discarded(bucket.unwrap_or_else(|| "None".to_string()));
            None
        }
    }
}

impl<C, I> TaskTransform<Event> for Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    fn transform(
        self: Box<Self>,
        input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let limiter = self.start_rate_limiter();
        Box::pin(input_rx.filter_map(move |event| {
            future::ready(self.throttle_one(&limiter, event, |event| self.render_key(event)))
        }))
    }
}

impl<C, I> KeyedTransform for Throttle<C, I>
where
    C: clock::Clock<Instant = I> + Clone + Send + Sync + 'static,
    I: clock::Reference + Send + 'static,
{
    type Key = ThrottleKey;

    fn key(&self, event: &Event) -> ThrottleKey {
        self.render_key(event)
    }

    fn transform_keyed(
        self: Box<Self>,
        input: BoxStream<'static, (ThrottleKey, Event)>,
    ) -> BoxStream<'static, Event> {
        let limiter = self.start_rate_limiter();
        input
            .filter_map(move |(key, event)| {
                future::ready(self.throttle_one(&limiter, event, |_| key))
            })
            .boxed()
    }
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("`threshold`, and `window_secs` must be non-zero"))]
    NonZero,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, task::Poll};

    use futures::SinkExt;
    use indoc::indoc;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vrl::event_path;

    use super::*;
    use crate::{
        event::LogEvent,
        test_util::components::assert_transform_compliance,
        transforms::{Transform, partitioned::Partitioned, test::create_topology},
    };

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
                window_secs: Duration::from_secs_f64(1.0),
                key_field: None,
                exclude: None,
                internal_metrics: ThrottleInternalMetricsConfig::default(),
                concurrency: std::num::NonZeroUsize::MIN,
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

    /// Runs `events` through a throttle built from `config`, in `shards` partitioned tasks, with a
    /// clock that never advances.
    async fn run_partitioned(
        config: &ThrottleConfig,
        shards: usize,
        events: Vec<Event>,
    ) -> Vec<Event> {
        let throttle = Throttle::new(
            config,
            &TransformContext::default(),
            clock::FakeRelativeClock::default(),
        )
        .unwrap();
        let shards = std::num::NonZeroUsize::new(shards).unwrap();
        Box::new(Partitioned::new(move || throttle.clone(), shards))
            .transform(futures::stream::iter(events).boxed())
            .collect()
            .await
    }

    /// The `sequence` of the events of each bucket, in output order.
    fn per_bucket(events: &[Event]) -> BTreeMap<String, Vec<i64>> {
        let mut buckets = BTreeMap::<String, Vec<i64>>::new();
        for event in events {
            let log = event.as_log();
            let bucket = log
                .get(event_path!("bucket"))
                .map_or_else(String::new, |b| b.to_string_lossy().into_owned());
            let sequence = log["sequence"].as_integer().unwrap();
            buckets.entry(bucket).or_default().push(sequence);
        }
        buckets
    }

    #[tokio::test]
    async fn partitioned_throttles_each_bucket_like_a_single_task() {
        let config = serde_yaml::from_str::<ThrottleConfig>(indoc! {r#"
            threshold: 3
            window_secs: 60
            key_field: "{{ bucket }}"
            exclude: "exists(.special)"
        "#})
        .unwrap();

        // 17 buckets, a few events without `bucket` (all in the `None` bucket), and a few excluded
        // events that are never throttled.
        let events: Vec<Event> = (0..600)
            .map(|sequence| {
                let mut log = LogEvent::default();
                log.insert(event_path!("sequence"), sequence);
                if sequence % 31 != 0 {
                    log.insert(event_path!("bucket"), format!("b{}", sequence % 17));
                }
                if sequence % 97 == 0 {
                    log.insert(event_path!("special"), true);
                }
                log.into()
            })
            .collect();

        let single = run_partitioned(&config, 1, events.clone()).await;
        let sharded = run_partitioned(&config, 4, events).await;

        let single = per_bucket(&single);
        assert_eq!(per_bucket(&sharded), single);

        // Sanity check of the expected output: 3 events per bucket, plus the excluded ones.
        assert_eq!(single["b1"], vec![1, 18, 35]);
        assert_eq!(
            single[""],
            vec![0, 31, 62, 93],
            "0 is excluded and uses no quota"
        );
        assert_eq!(single["b12"], vec![12, 29, 46, 97], "97 is excluded");
    }
}
