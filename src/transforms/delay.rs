use std::{num::NonZeroUsize, pin::Pin, time::Duration};

use async_stream::stream;
use futures::{Stream, StreamExt};
use serde_with::serde_as;
use tokio_util::time::DelayQueue;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::INTENTIONAL;
use vector_lib::{config::clone_input_definitions, internal_event::ComponentEventsDropped};

use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    event::Event,
    schema,
    transforms::{TaskTransform, Transform},
};

/// Configuration for the `delay` transform.
#[serde_as]
#[configurable_component(transform("delay", "Slow down events passing through a topology."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DelayConfig {
    /// Time to delay each event, in milliseconds.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Delay in milliseconds", docs::example = 200))]
    delay_milliseconds: Duration,

    /// Optional limit for number of items in the delay queue.
    queue_capacity: Option<NonZeroUsize>,

    /// Strategy to handle full queue capacity.
    #[serde(default)]
    overflow_strategy: OverflowStrategy,

    /// Delay events in provided delay periods until the condition is met.
    delay_until_condition: Option<AnyCondition>,
}

/// Event handling behavior when delay queue is full.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OverflowStrategy {
    /// Wait for free space in the queue.
    ///
    /// This applies backpressure up the topology, signalling that sources should slow down
    /// the acceptance/consumption of events. This may cause the system to degenerate if this
    /// component blocks for too long.
    #[default]
    Block,

    /// Drops the event instead of waiting for free space in the queue.
    ///
    /// The event will be intentionally dropped. This mode is typically used when performance is the
    /// highest priority, and it is preferable to temporarily lose events rather than cause a
    /// slowdown in the acceptance/consumption of events.
    DropNewest,

    /// Forward the event without any delay to next component.
    Forward,
}

impl_generate_config_from_default!(DelayConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "delay")]
impl TransformConfig for DelayConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::event_task(Delay::new(self, context)?))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        // The event is not modified, so the definition is passed through as-is
        vec![TransformOutput::new(
            DataType::all_bits(),
            clone_input_definitions(input_definitions),
        )]
    }
}

pub struct Delay {
    delay: Duration,
    queue: DelayQueue<Event>,
    queue_capacity: Option<NonZeroUsize>,
    overflow_strategy: OverflowStrategy,
    delay_until_condition: Option<Condition>,
}

impl Delay {
    pub fn new(config: &DelayConfig, context: &TransformContext) -> crate::Result<Self> {
        Ok(Self {
            delay: config.delay_milliseconds,
            queue: config
                .queue_capacity
                .map(|c| DelayQueue::with_capacity(c.into()))
                .unwrap_or_default(),
            queue_capacity: config.queue_capacity,
            overflow_strategy: config.overflow_strategy,
            delay_until_condition: config
                .delay_until_condition
                .as_ref()
                .map(|c| c.build(&context.enrichment_tables, &context.metrics_storage))
                .transpose()?,
        })
    }
}

impl TaskTransform<Event> for Delay {
    fn transform(
        mut self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        Box::pin(stream! {
            let mut done = false;
            loop {
                if done && self.queue.is_empty() {
                    break;
                }
                tokio::select! {
                    maybe_event = input_rx.next(), if !done => {
                        match maybe_event {
                            None => {
                                done = true;
                            }
                            Some(event) => {
                                let (result, event) = if let Some(condition) = self.delay_until_condition.as_ref() {
                                    condition.check(event)
                                } else {
                                    // We need to have 1 delay when no condition is set, put it in
                                    // the queue
                                    (false, event)
                                };
                                if result {
                                    yield event
                                } else {
                                    if let Some(capacity) = self.queue_capacity && capacity.get() <= self.queue.len() {
                                        match self.overflow_strategy {
                                            OverflowStrategy::Block => {
                                                while capacity.get() <= self.queue.len() && let Some(next) = self.queue.next().await {
                                                    let event = next.into_inner();
                                                    let (result, event) = if let Some(condition) = self.delay_until_condition.as_ref() {
                                                        condition.check(event)
                                                    } else {
                                                        // One delay is done, pass the event further
                                                        (true, event)
                                                    };
                                                    if result {
                                                        yield event;
                                                    } else {
                                                        self.queue.insert(event, self.delay);
                                                    }
                                                }
                                            },
                                            OverflowStrategy::DropNewest => {
                                                emit!(ComponentEventsDropped::<INTENTIONAL> {
                                                    count: 1,
                                                    reason: "Queue is full and overflow strategy is drop_newest",
                                                });
                                                continue;
                                            }
                                            OverflowStrategy::Forward => {
                                                yield event;
                                                continue;
                                            }
                                        }
                                    }
                                    self.queue.insert(event, self.delay);
                                }
                            }
                        }
                    },
                    Some(res) = self.queue.next() => {
                        let event = res.into_inner();
                        let (result, event) = if let Some(condition) = self.delay_until_condition.as_ref() {
                            condition.check(event)
                        } else {
                            // One delay is done, pass the event further
                            (true, event)
                        };
                        if result {
                            yield event;
                        } else {
                            self.queue.insert(event, self.delay);
                        }
                        if done && self.queue.is_empty() {
                            break;
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use futures::SinkExt;
    use vector_lib::event::TraceEvent;

    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DelayConfig>();
    }

    #[tokio::test]
    async fn delay_events() {
        let config = toml::from_str::<DelayConfig>(
            r"
delay_milliseconds = 200
",
        )
        .unwrap();

        let delay =
            Transform::event_task(Delay::new(&config, &TransformContext::default()).unwrap());

        let delay = delay.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = delay.transform_events(Box::pin(rx));

        tx.send(LogEvent::default().into()).await.unwrap();

        // We should be pending, because we are now waiting for the delay
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        // Wait long enough for delay to end
        tokio::time::sleep(Duration::from_secs_f64(0.3)).await;

        if !matches!(futures::poll!(out_stream.next()), Poll::Ready(Some(_event))) {
            panic!("Unexpectedly received None or Pending in output stream");
        }
    }

    #[tokio::test]
    async fn delay_events_at_capacity_drop_newest() {
        let config = toml::from_str::<DelayConfig>(
            r#"
delay_milliseconds = 200
queue_capacity = 1
overflow_strategy = "drop_newest"
"#,
        )
        .unwrap();

        let delay =
            Transform::event_task(Delay::new(&config, &TransformContext::default()).unwrap());

        let delay = delay.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = delay.transform_events(Box::pin(rx));

        tx.send(LogEvent::default().into()).await.unwrap();
        tx.send(TraceEvent::default().into()).await.unwrap();

        // We should be pending, because we are now waiting for the delay
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        // Wait long enough for delay to end
        tokio::time::sleep(Duration::from_secs_f64(0.3)).await;

        let Poll::Ready(Some(event)) = futures::poll!(out_stream.next()) else {
            panic!("Unexpectedly received None or Pending in output stream");
        };
        assert!(event.try_into_log().is_some());

        // We should be pending, because trace event should have been dropped
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));
    }

    #[tokio::test]
    async fn delay_events_at_capacity_pass() {
        let config = toml::from_str::<DelayConfig>(
            r#"
delay_milliseconds = 200
queue_capacity = 1
overflow_strategy = "forward"
"#,
        )
        .unwrap();

        let delay =
            Transform::event_task(Delay::new(&config, &TransformContext::default()).unwrap());

        let delay = delay.into_task();

        let (mut tx, rx) = futures::channel::mpsc::channel(10);
        let mut out_stream = delay.transform_events(Box::pin(rx));

        tx.send(LogEvent::default().into()).await.unwrap();
        tx.send(TraceEvent::default().into()).await.unwrap();

        // First event should be trace, because it is passed right away before delay
        let Poll::Ready(Some(event)) = futures::poll!(out_stream.next()) else {
            panic!("Unexpectedly received None or Pending in output stream");
        };
        assert!(event.try_into_trace().is_some());

        // We should be pending, because we are now waiting for the delay
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        // Wait long enough for delay to end
        tokio::time::sleep(Duration::from_secs_f64(0.3)).await;

        let Poll::Ready(Some(event)) = futures::poll!(out_stream.next()) else {
            panic!("Unexpectedly received None or Pending in output stream");
        };
        assert!(event.try_into_log().is_some());
    }
}
