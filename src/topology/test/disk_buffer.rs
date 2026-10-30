use std::{
    num::NonZeroU64,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use tokio::sync::{mpsc, oneshot};
use tokio_test::{assert_pending, task::spawn};
use vector_lib::{
    buffers::{BufferConfig, BufferType, WhenFull},
    config::{AcknowledgementsConfig, Input},
    configurable::configurable_component,
    event::{Event, LogEvent},
    finalization::{EventFinalizers, EventStatus, Finalizable as _},
    lookup::event_path,
    partition::Partitioner,
    sink::{StreamSink, VectorSink},
    stream::{BatcherSettings, PartitionedBatcher},
};

use super::into_message;
use crate::{
    SourceSender,
    config::{Config, SinkConfig, SinkContext, SinkOuter, ValidatedSink},
    sinks::Healthcheck,
    test_util::{mock::basic_source, start_topology},
    topology::RunningTopology,
};

/// Test sink for exercising batching and finalization at the topology boundary.
#[configurable_component(sink("test_shutdown_batcher", "Test (shutdown batcher)."))]
#[derive(Clone, Debug, Default)]
struct ShutdownSinkConfig {
    #[serde(skip)]
    sink: Arc<Mutex<Option<ShutdownSink>>>,
}

impl_generate_config_from_default!(ShutdownSinkConfig);

#[async_trait]
#[typetag::serde(name = "test_shutdown_batcher")]
impl SinkConfig for ShutdownSinkConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

#[async_trait]
impl ValidatedSink for ShutdownSinkConfig {
    type Validated = ();

    fn validate(&self) -> crate::Result<Self::Validated> {
        Ok(())
    }

    async fn build(
        &self,
        _validated: &Self::Validated,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = self.sink.lock().unwrap().take().unwrap();
        Ok((
            VectorSink::from_event_streamsink(sink),
            Box::pin(async { Ok(()) }),
        ))
    }
}

#[derive(Debug)]
enum ShutdownSink {
    Batch {
        ingested: mpsc::UnboundedSender<()>,
        output: mpsc::UnboundedSender<(String, Vec<Event>)>,
    },
    HoldFinalizers {
        finalizers: mpsc::UnboundedSender<EventFinalizers>,
        eof: oneshot::Sender<()>,
    },
}

struct TestPartitioner;

impl Partitioner for TestPartitioner {
    type Item = Event;
    type Key = String;

    fn partition(&self, item: &Event) -> String {
        item.as_log()
            .get(event_path!("partition"))
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }
}

#[async_trait]
impl StreamSink<Event> for ShutdownSink {
    async fn run(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        match *self {
            Self::Batch { ingested, output } => {
                let settings = BatcherSettings {
                    timeout: Duration::from_mins(5),
                    size_limit: 1024 * 1024,
                    item_limit: 100,
                };
                let input = input.inspect(move |_| ingested.send(()).unwrap());
                let mut batches =
                    PartitionedBatcher::new(input, TestPartitioner, settings.timeout, move |_| {
                        settings.as_byte_size_config()
                    });
                while let Some((key, mut batch)) = batches.next().await {
                    for event in &mut batch {
                        event
                            .take_finalizers()
                            .update_status(EventStatus::Delivered);
                    }
                    output.send((key, batch)).unwrap();
                }
            }
            Self::HoldFinalizers { finalizers, eof } => {
                while let Some(mut event) = input.next().await {
                    finalizers.send(event.take_finalizers()).unwrap();
                }
                eof.send(()).unwrap();
            }
        }
        Ok(())
    }
}

async fn start_disk_topology(dir: &Path, sink: ShutdownSink) -> (SourceSender, RunningTopology) {
    crate::test_util::trace_init();
    let (input, source) = basic_source();
    let mut config = Config::builder();
    config.set_data_dir(dir);
    config.graceful_shutdown_duration = Some(Duration::from_secs(2));
    config.add_source("in", source);
    let mut sink = SinkOuter::new(
        vec![String::from("in")],
        ShutdownSinkConfig {
            sink: Arc::new(Mutex::new(Some(sink))),
        },
    );
    sink.buffer = BufferConfig::Single(BufferType::DiskV2 {
        max_size: NonZeroU64::new(268_435_488).unwrap(),
        when_full: WhenFull::Block,
    });
    config.add_sink_outer("out", sink);
    let (topology, _) = start_topology(config.build().unwrap(), false).await;
    (input, topology)
}

#[tokio::test]
async fn topology_disk_buffer_flushes_partial_partitions_on_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let (ingested_tx, mut ingested) = mpsc::unbounded_channel();
    let (output, mut batches) = mpsc::unbounded_channel();
    let (mut input, topology) = start_disk_topology(
        dir.path(),
        ShutdownSink::Batch {
            ingested: ingested_tx,
            output,
        },
    )
    .await;
    let expected = [("a", "a0"), ("b", "b0"), ("a", "a1"), ("b", "b1")];
    for (partition, id) in expected {
        let mut event = LogEvent::from(id);
        event.insert(event_path!("partition"), partition);
        input.send_event(Event::Log(event)).await.unwrap();
    }
    // Every event must reach the batcher before shutdown, while both batches remain partial.
    for _ in expected {
        tokio::time::timeout(Duration::from_secs(5), ingested.recv())
            .await
            .unwrap()
            .unwrap();
    }
    assert!(batches.try_recv().is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(5), topology.stop())
            .await
            .unwrap()
    );

    let mut partitions = Vec::new();
    let mut messages = Vec::new();
    while let Some((key, batch)) = batches.recv().await {
        partitions.push(key);
        messages.extend(batch.into_iter().map(into_message));
    }
    partitions.sort();
    messages.sort();
    assert_eq!(partitions, ["a", "b"]);
    assert_eq!(messages, ["a0", "a1", "b0", "b1"]);
}

#[tokio::test]
async fn topology_disk_buffer_waits_for_completion_after_sink_returns() {
    for release_before_deadline in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (finalizers, mut held_finalizers) = mpsc::unbounded_channel();
        let (eof_tx, eof) = oneshot::channel();
        let (mut input, topology) = start_disk_topology(
            dir.path(),
            ShutdownSink::HoldFinalizers {
                finalizers,
                eof: eof_tx,
            },
        )
        .await;
        input
            .send_event(Event::Log(LogEvent::from("held")))
            .await
            .unwrap();
        let finalizers = tokio::time::timeout(Duration::from_secs(5), held_finalizers.recv())
            .await
            .unwrap()
            .unwrap();
        let mut shutdown = spawn(topology.stop());
        assert_pending!(shutdown.poll());
        tokio::time::timeout(Duration::from_secs(1), eof)
            .await
            .unwrap()
            .unwrap();
        assert_pending!(shutdown.poll());
        if release_before_deadline {
            finalizers.update_status(EventStatus::Delivered);
            drop(finalizers);
            assert!(
                tokio::time::timeout(Duration::from_secs(5), shutdown)
                    .await
                    .unwrap()
            );
        } else {
            assert!(
                !tokio::time::timeout(Duration::from_secs(5), shutdown)
                    .await
                    .unwrap()
            );
            drop(finalizers);
        }
    }
}
