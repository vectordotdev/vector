use std::{pin::Pin, sync::Mutex};

use async_trait::async_trait;
use futures_util::{stream::BoxStream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::{channel, Receiver, Sender};
use vector_buffers::Acker;
use vector_core::{
    config::{AcknowledgementsConfig, DataType, Input, Output},
    event::{Event, EventArray, EventContainer},
    schema::Definition,
    sink::{StreamSink, VectorSink},
    source::Source,
    transform::{
        FunctionTransform, OutputBuffer, TaskTransform, Transform, TransformConfig,
        TransformContext,
    },
};

use crate::{
    config::{ConfigBuilder, SinkConfig, SinkContext, SourceConfig, SourceContext},
    sinks::Healthcheck,
    test_util::{components::assert_transform_compliance, start_topology},
    topology::RunningTopology,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
enum TransformType {
    Function,
    Synchronous,
    Task,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NoopTransformConfig {
    transform_type: TransformType,
}

#[async_trait]
#[typetag::serde(name = "noop")]
impl TransformConfig for NoopTransformConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn transform_type(&self) -> &'static str {
        "noop"
    }

    async fn build(&self, _: &TransformContext) -> crate::Result<Transform> {
        match self.transform_type {
            TransformType::Function => Ok(Transform::Function(Box::new(NoopTransform))),
            TransformType::Synchronous => Ok(Transform::Synchronous(Box::new(NoopTransform))),
            TransformType::Task => Ok(Transform::Task(Box::new(NoopTransform))),
        }
    }
}

impl From<TransformType> for NoopTransformConfig {
    fn from(transform_type: TransformType) -> Self {
        Self { transform_type }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OneshotSourceConfig {
    #[serde(skip)]
    event: Option<Event>,
}

#[async_trait]
#[typetag::serde(name = "oneshot")]
impl SourceConfig for OneshotSourceConfig {
    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn source_type(&self) -> &'static str {
        "oneshot"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }

    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let event = self.event.clone();
        let mut out = cx.out;
        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let event = event.expect("event must not be none");
            out.send_event(event).await.unwrap();
            shutdown.await;
            Ok(())
        }))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OneshotSinkConfig {
    #[serde(skip)]
    tx: Mutex<Option<Sender<EventArray>>>,
}

#[async_trait]
#[typetag::serde(name = "oneshot")]
impl SinkConfig for OneshotSinkConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn sink_type(&self) -> &'static str {
        "oneshot"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }

    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tx = {
            let mut guard = self.tx.lock().expect("who cares if the lock is poisoned");
            guard.take()
        };
        let sink = Box::new(OneshotSink {
            acker: cx.acker,
            tx,
        });

        let healthcheck = Box::pin(async { Ok(()) });

        Ok((VectorSink::Stream(sink), healthcheck))
    }
}

struct OneshotSink {
    acker: Acker,
    tx: Option<Sender<EventArray>>,
}

#[async_trait]
impl StreamSink<EventArray> for OneshotSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, EventArray>) -> Result<(), ()> {
        let tx = self.tx.take().expect("cannot take rx more than once");
        let events = input
            .next()
            .await
            .expect("must always get an item in oneshot sink");
        self.acker.ack(events.len());
        let _ = tx.send(events);

        Ok(())
    }
}

#[derive(Clone)]
struct NoopTransform;

impl FunctionTransform for NoopTransform {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        output.push(event);
    }
}

impl<T> TaskTransform<T> for NoopTransform
where
    T: EventContainer + 'static,
{
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn futures_util::Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>> {
        Box::pin(task)
    }
}

async fn create_topology(
    event: Event,
    transform_type: TransformType,
) -> (RunningTopology, Receiver<EventArray>) {
    let mut builder = ConfigBuilder::default();

    let (tx, rx) = channel();

    builder.add_source("in", OneshotSourceConfig { event: Some(event) });
    builder.add_transform(
        "transform",
        &["in"],
        NoopTransformConfig::from(transform_type),
    );
    builder.add_sink(
        "out",
        &["transform"],
        OneshotSinkConfig {
            tx: Mutex::new(Some(tx)),
        },
    );

    let config = builder.build().expect("building config should not fail");
    let (topology, _) = start_topology(config, false).await;

    (topology, rx)
}

#[tokio::test]
async fn test_function_transform_single_event() {
    assert_transform_compliance(async {
        let original_event = Event::from("function transform being tested");

        let (topology, rx) = create_topology(original_event.clone(), TransformType::Function).await;
        topology.stop().await;

        let events = rx.await.expect("must get back event from rx");
        let mut events = events.into_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);

        let event = events.remove(0);
        assert_eq!(original_event, event);
    })
    .await;
}

#[tokio::test]
async fn test_sync_transform_single_event() {
    assert_transform_compliance(async {
        let original_event = Event::from("function transform being tested");

        let (topology, rx) =
            create_topology(original_event.clone(), TransformType::Synchronous).await;
        topology.stop().await;

        let events = rx.await.expect("must get back event from rx");
        let mut events = events.into_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);

        let event = events.remove(0);
        assert_eq!(original_event, event);
    })
    .await;
}

#[tokio::test]
async fn test_task_transform_single_event() {
    assert_transform_compliance(async {
        let original_event = Event::from("function transform being tested");

        let (topology, rx) = create_topology(original_event.clone(), TransformType::Task).await;
        topology.stop().await;

        let events = rx.await.expect("must get back event from rx");
        let mut events = events.into_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1);

        let event = events.remove(0);
        assert_eq!(original_event, event);
    })
    .await;
}
