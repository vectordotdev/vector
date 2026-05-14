use std::{pin::Pin, time::Duration};

use async_stream::stream;
use futures::{Stream, StreamExt};
use serde_with::serde_as;
use tokio_util::time::DelayQueue;
use vector_lib::config::clone_input_definitions;
use vector_lib::configurable::configurable_component;

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
    /// Time to delay each event, in seconds.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Delay per event", docs::example = 0.2))]
    delay_per_event: Duration,

    /// Delay only events matched by this condition.
    condition: Option<AnyCondition>,
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
    condition: Condition,
}

impl Delay {
    pub fn new(config: &DelayConfig, context: &TransformContext) -> crate::Result<Self> {
        Ok(Self {
            delay: config.delay_per_event,
            queue: DelayQueue::new(),
            condition: config
                .condition
                .as_ref()
                .map(|c| c.build(&context.enrichment_tables, &context.metrics_storage))
                .transpose()?
                .unwrap_or(Condition::AlwaysPass),
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
                tokio::select! {
                    maybe_event = input_rx.next(), if !done => {
                        match maybe_event {
                            None => {
                                done = true;
                            }
                            Some(event) => {
                                let (result, event) = self.condition.check(event);
                                if result {
                                    self.queue.insert(event, self.delay);
                                } else {
                                    yield event;
                                }
                            }
                        }
                    },
                    Some(res) = self.queue.next() => {
                        yield res.into_inner();
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
delay_per_event = 0.2
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
    async fn delay_events_condition() {
        let config = toml::from_str::<DelayConfig>(
            r#"
delay_per_event = 0.2
condition.type = "is_log"
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

        let Poll::Ready(Some(event)) = futures::poll!(out_stream.next()) else {
            panic!("Unexpectedly received None or Pending in output stream");
        };
        assert!(event.try_into_trace().is_some());

        // We should be pending, because we are now waiting for the delay
        assert_eq!(Poll::Pending, futures::poll!(out_stream.next()));

        // Wait long enough for delay to end
        tokio::time::sleep(Duration::from_secs_f64(0.3)).await;

        if !matches!(futures::poll!(out_stream.next()), Poll::Ready(Some(_event))) {
            panic!("Unexpectedly received None or Pending in output stream");
        }
    }
}
