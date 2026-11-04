use std::{hash::Hash, num::NonZeroU32, pin::Pin, time::Duration};

use async_stream::stream;
use futures::{Stream, StreamExt};
use governor::{Quota, clock};
use metrics::Counter;
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
    template::UnconfinedTemplate,
    transforms::TaskTransform,
};

#[derive(Clone)]
pub struct Throttle<C: clock::Clock<Instant = I>, I: clock::Reference> {
    pub quota: Quota,
    pub flush_keys_interval: Duration,
    key_field: Option<UnconfinedTemplate>,
    exclude: Option<Condition>,
    pub clock: C,
    internal_metrics: ThrottleInternalMetricsConfig,
    pub cpu_ns: Option<Counter>,
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
        config.internal_metrics.validate()?;

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
            cpu_ns: context.cpu_ns.clone(),
        })
    }

    #[must_use]
    pub fn start_rate_limiter<K>(&self) -> RateLimiterRunner<K, C>
    where
        K: Hash + Eq + Clone + Send + Sync + 'static,
    {
        RateLimiterRunner::start(self)
    }

    pub fn emit_event_discarded(&self, key: String, event: &Event) {
        let tags = match &self.internal_metrics.events_discarded_tags {
            Some(templates) => Some(
                templates
                    .iter()
                    .filter_map(|(name, template)| {
                        template
                            .render_string(event)
                            .map_err(|error| {
                                emit!(TemplateRenderingError {
                                    error,
                                    field: Some(name),
                                    drop_event: false,
                                });
                            })
                            .ok()
                            .filter(|value| !value.is_empty())
                            .map(|value| (name.clone(), value))
                    })
                    .collect(),
            ),
            None => self
                .internal_metrics
                .emit_events_discarded_per_key
                .then(|| vec![("key".to_owned(), key.clone())]),
        };
        emit!(ThrottleEventDiscarded {
            key,
            tags,
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
                                emit!(TemplateRenderingError {
                                    error,
                                    field: Some("key_field"),
                                    drop_event: false,
                                });
                            })
                            .ok()
                    });

                    if limiter.check_key(&key) {
                        Some(event)
                    } else {
                        self.emit_event_discarded(key.unwrap_or_else(|| "None".to_string()), &event);
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
    #[snafu(display("`threshold`, and `window_secs` must be non-zero"))]
    NonZero,
    #[snafu(display(
        "`internal_metrics.events_discarded_tags` cannot override reserved tag `{tag}`"
    ))]
    ReservedMetricTag { tag: String },
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, task::Poll};

    use futures::SinkExt;
    use indoc::indoc;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vrl::event_path;

    use super::*;
    use crate::{
        config::TransformConfig,
        event::LogEvent,
        test_util::components::assert_transform_compliance,
        transforms::{Transform, test::create_topology},
    };

    async fn discard_metrics_case(emit_per_key: bool, extra_tags: bool) {
        vector_lib::metrics::init_test();
        vector_lib::event_test_util::clear_recorded_events();
        let mut config: ThrottleConfig = serde_yaml::from_str(indoc! {r#"
            threshold: 1
            window_secs: 60
            key_field: "{{ entity }}"
        "#})
        .unwrap();
        config.internal_metrics.emit_events_discarded_per_key = emit_per_key;
        if extra_tags {
            config.internal_metrics.events_discarded_tags = Some(HashMap::from([
                ("tenant".to_owned(), "{{ tenant }}".try_into().unwrap()),
                ("key".to_owned(), "{{ region }}".try_into().unwrap()),
            ]));
        }
        config.validate_structure().unwrap();
        let throttle = Throttle::new(
            &config,
            &TransformContext::default(),
            clock::FakeRelativeClock::default(),
        )
        .unwrap();
        let events = [
            Some("first"),
            Some("second"),
            Some("second"),
            Some("third"),
            None,
            Some(""),
        ]
        .into_iter()
        .map(|tenant| {
            let mut event = LogEvent::default();
            event.insert(event_path!("entity"), "shared-bucket");
            event.insert(event_path!("region"), "east");
            if let Some(tenant) = tenant {
                event.insert(event_path!("tenant"), tenant);
            }
            Event::from(event)
        });
        let output = Box::new(throttle)
            .transform(Box::pin(futures::stream::iter(events)))
            .collect::<Vec<_>>()
            .await;
        // Tag changes cannot grant another allowance in the same entity's bucket.
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(event_path!("tenant")),
            Some(&"first".into())
        );

        assert_eq!(
            vector_lib::event_test_util::contains_name_once("TemplateRenderingError").is_ok(),
            extra_tags,
        );
        let metrics = vector_lib::metrics::Controller::get()
            .unwrap()
            .capture_metrics();
        assert!(
            metrics
                .iter()
                .all(|metric| metric.name() != "component_errors_total")
        );
        let mut discards = metrics
            .iter()
            .filter(|metric| metric.name() == "events_discarded_total")
            .map(|metric| {
                assert_eq!(
                    metric.tag_value("key").as_deref(),
                    Some(if extra_tags { "east" } else { "shared-bucket" })
                );
                let crate::event::MetricValue::Counter { value } = metric.value() else {
                    panic!("expected counter");
                };
                (metric.tag_value("tenant"), *value)
            })
            .collect::<Vec<_>>();
        discards.sort_by(|left, right| left.0.cmp(&right.0));
        let expected = match (emit_per_key, extra_tags) {
            (false, false) => vec![],
            (true, false) => vec![(None, 5.0)],
            (_, true) => vec![
                (None, 2.0),
                (Some("second".to_owned()), 2.0),
                (Some("third".to_owned()), 1.0),
            ],
        };
        assert_eq!(discards, expected);
        let aggregate = metrics
            .iter()
            .find(|metric| metric.name() == "component_discarded_events_total")
            .unwrap();
        assert_eq!(
            aggregate.value(),
            &crate::event::MetricValue::Counter { value: 5.0 }
        );
        assert_eq!(aggregate.tag_value("tenant"), None);
        assert_eq!(aggregate.tag_value("key"), None);
    }

    #[tokio::test]
    async fn discard_tags_preserves_bucket_and_counts_missing_fields() {
        discard_metrics_case(true, true).await;
    }

    #[tokio::test]
    async fn discard_tags_disabled_preserves_aggregate() {
        discard_metrics_case(false, false).await;
    }

    #[tokio::test]
    async fn discard_tags_enable_counter_without_legacy_flag() {
        discard_metrics_case(false, true).await;
    }

    #[tokio::test]
    async fn discard_tags_unconfigured_preserves_existing_labels() {
        discard_metrics_case(true, false).await;
    }

    #[rstest::rstest]
    #[tokio::test]
    async fn discard_tags_without_key_aggregate_across_buckets(
        #[values(false, true)] legacy_enabled: bool,
        #[values(false, true)] extra_tags: bool,
    ) {
        vector_lib::metrics::init_test();
        let mut config: ThrottleConfig = serde_yaml::from_str(indoc! {r#"
            threshold: 1
            window_secs: 60
            key_field: "{{ entity }}"
            internal_metrics:
              events_discarded_tags: {}
              include_group_tag: true
        "#})
        .unwrap();
        config.internal_metrics.emit_events_discarded_per_key = legacy_enabled;
        if extra_tags {
            config.internal_metrics.events_discarded_tags =
                Some([("tenant".to_owned(), "{{ tenant }}".try_into().unwrap())].into());
        }
        config.validate_structure().unwrap();
        let throttle = Throttle::new(
            &config,
            &TransformContext::default(),
            clock::FakeRelativeClock::default(),
        )
        .unwrap();
        let events = ["a", "a", "a", "b", "b", "b"].map(|entity| {
            let mut event = LogEvent::default();
            event.insert(event_path!("entity"), entity);
            event.insert(event_path!("tenant"), "shared");
            Event::from(event)
        });
        let output = Box::new(throttle)
            .transform(Box::pin(futures::stream::iter(events)))
            .collect::<Vec<_>>()
            .await;
        assert_eq!(
            output
                .iter()
                .map(|event| event
                    .as_log()
                    .get(event_path!("entity"))
                    .unwrap()
                    .to_string_lossy())
                .collect::<Vec<_>>(),
            ["a", "b"],
        );
        let metrics = vector_lib::metrics::Controller::get()
            .unwrap()
            .capture_metrics();
        let discards = metrics
            .iter()
            .filter(|metric| metric.name() == "events_discarded_total")
            .collect::<Vec<_>>();
        assert_eq!(discards.len(), 1);
        assert_eq!(discards[0].tag_value("key"), None);
        assert_eq!(
            discards[0].tag_value("tenant").as_deref(),
            extra_tags.then_some("shared")
        );
        assert_eq!(
            discards[0].value(),
            &crate::event::MetricValue::Counter { value: 4.0 }
        );

        let grouped = metrics
            .iter()
            .filter(|metric| metric.name() == "component_discarded_events_total")
            .collect::<Vec<_>>();
        assert_eq!(grouped.len(), 2);
        for group in ["a", "b"] {
            let metric = grouped
                .iter()
                .find(|metric| metric.tag_value("group").as_deref() == Some(group))
                .unwrap();
            assert_eq!(
                metric.value(),
                &crate::event::MetricValue::Counter { value: 2.0 }
            );
            assert_eq!(metric.tag_value("tenant"), None);
        }
    }

    #[test]
    fn discard_tags_cannot_override_builtin_tags() {
        for &tag in vector_lib::metrics::LABELS.iter() {
            let config = ThrottleConfig {
                threshold: 1,
                window_secs: Duration::from_secs(1),
                internal_metrics: ThrottleInternalMetricsConfig {
                    events_discarded_tags: Some(
                        [(tag.to_owned(), "override".try_into().unwrap())].into(),
                    ),
                    ..Default::default()
                },
                ..Default::default()
            };
            let validation_errors = config
                .validate_structure()
                .expect_err("reserved metric tag must fail structural validation");
            let error = Throttle::new(
                &config,
                &TransformContext::default(),
                clock::FakeRelativeClock::default(),
            )
            .err()
            .expect("reserved metric tag must be rejected");
            assert!(error.to_string().contains(&format!("reserved tag `{tag}`")));
            assert_eq!(validation_errors, vec![error.to_string()]);
        }
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
