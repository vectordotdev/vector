// Using a shared mod like this is probably not the best idea, since we have to
// disable the `dead_code` lint, as we don't need all of the helpers from here
// all over the place.
#![allow(clippy::type_complexity)]
#![allow(dead_code)]

use std::{
    collections::BTreeSet,
    fs::{create_dir, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use futures::{stream::BoxStream, FutureExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tokio::sync::oneshot;
use tracing::{error, info};
use vector::{
    config::{
        AcknowledgementsConfig, DataType, Input, Output, SinkConfig, SinkContext, SourceConfig,
        SourceContext, TransformConfig, TransformContext,
    },
    event::{
        metric::{self, MetricData, MetricValue},
        Event, EventArray, EventContainer, Value,
    },
    schema,
    sinks::{util::StreamSink, Healthcheck, VectorSink},
    source_sender::SourceSender,
    sources::Source,
    test_util::{temp_dir, temp_file},
    transforms::{FunctionTransform, OutputBuffer, Transform},
};
use vector_buffers::{topology::channel::LimitedReceiver, Acker};

pub fn sink(channel_size: usize) -> (impl Stream<Item = EventArray>, MockSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = MockSinkConfig::new(tx, true);
    (rx.into_stream(), sink)
}

pub fn sink_with_data(
    channel_size: usize,
    data: &str,
) -> (impl Stream<Item = EventArray>, MockSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = MockSinkConfig::new_with_data(tx, true, data);
    (rx.into_stream(), sink)
}

pub fn sink_failing_healthcheck(
    channel_size: usize,
) -> (impl Stream<Item = EventArray>, MockSinkConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(channel_size);
    let sink = MockSinkConfig::new(tx, false);
    (rx.into_stream(), sink)
}

pub fn sink_dead() -> MockSinkConfig {
    MockSinkConfig::new_dead(false)
}

pub fn source() -> (SourceSender, MockSourceConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = MockSourceConfig::new(rx);
    (tx, source)
}

pub fn source_with_data(data: &str) -> (SourceSender, MockSourceConfig) {
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = MockSourceConfig::new_with_data(rx, data);
    (tx, source)
}

pub fn source_with_event_counter() -> (SourceSender, MockSourceConfig, Arc<AtomicUsize>) {
    let event_counter = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = SourceSender::new_with_buffer(1);
    let source = MockSourceConfig::new_with_event_counter(rx, event_counter.clone());
    (tx, source, event_counter)
}

pub fn transform(suffix: &str, increase: f64) -> MockTransformConfig {
    MockTransformConfig::new(suffix.to_owned(), increase)
}

/// Creates a file with given content
pub fn create_file(config: &str) -> PathBuf {
    let path = temp_file();
    overwrite_file(path.clone(), config);
    path
}

/// Overwrites file with given content
pub fn overwrite_file(path: PathBuf, config: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
}

pub fn create_directory() -> PathBuf {
    let path = temp_dir();
    create_dir(path.clone()).unwrap();
    path
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MockSourceConfig {
    #[serde(skip)]
    receiver: Arc<Mutex<Option<LimitedReceiver<EventArray>>>>,
    #[serde(skip)]
    event_counter: Option<Arc<AtomicUsize>>,
    #[serde(skip)]
    data_type: Option<DataType>,
    #[serde(skip)]
    force_shutdown: bool,
    // something for serde to use, so we can trigger rebuilds
    data: Option<String>,
}

impl MockSourceConfig {
    pub fn new(receiver: LimitedReceiver<EventArray>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn new_with_data(receiver: LimitedReceiver<EventArray>, data: &str) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: Some(data.into()),
        }
    }

    pub fn new_with_event_counter(
        receiver: LimitedReceiver<EventArray>,
        event_counter: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: Some(event_counter),
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn set_data_type(&mut self, data_type: DataType) {
        self.data_type = Some(data_type)
    }

    pub fn set_force_shutdown(&mut self, force_shutdown: bool) {
        self.force_shutdown = force_shutdown;
    }
}

#[async_trait]
#[typetag::serde(name = "mock")]
impl SourceConfig for MockSourceConfig {
    async fn build(&self, cx: SourceContext) -> Result<Source, vector::Error> {
        let wrapped = self.receiver.clone();
        let event_counter = self.event_counter.clone();
        let mut recv = wrapped.lock().unwrap().take().unwrap();
        let shutdown1 = cx.shutdown.clone();
        let shutdown2 = cx.shutdown;
        let mut out = cx.out;
        let force_shutdown = self.force_shutdown;

        Ok(Box::pin(async move {
            tokio::pin!(shutdown1);
            tokio::pin!(shutdown2);

            loop {
                tokio::select! {
                    biased;

                    _ = &mut shutdown1, if force_shutdown => break,

                    Some(array) = recv.next() => {
                        if let Some(counter) = &event_counter {
                            counter.fetch_add(array.len(), Ordering::Relaxed);
                        }

                        if let Err(e) = out.send_event(array).await {
                            error!(message = "Error sending in sink..", %e);
                            return Err(())
                        }
                    },

                    _ = &mut shutdown2, if !force_shutdown => break,
                }
            }

            info!("Finished sending.");
            Ok(())
        }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.data_type.unwrap())]
    }

    fn source_type(&self) -> &'static str {
        "mock"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub struct MockTransform {
    suffix: String,
    increase: f64,
}

impl FunctionTransform for MockTransform {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        match &mut event {
            Event::Log(log) => {
                let mut v = log
                    .get(vector::config::log_schema().message_key())
                    .unwrap()
                    .to_string_lossy();
                v.push_str(&self.suffix);
                log.insert(vector::config::log_schema().message_key(), Value::from(v));
            }
            Event::Metric(metric) => {
                let increment = match metric.value() {
                    MetricValue::Counter { .. } => Some(MetricValue::Counter {
                        value: self.increase,
                    }),
                    MetricValue::Gauge { .. } => Some(MetricValue::Gauge {
                        value: self.increase,
                    }),
                    MetricValue::Distribution { statistic, .. } => {
                        Some(MetricValue::Distribution {
                            samples: vec![metric::Sample {
                                value: self.increase,
                                rate: 1,
                            }],
                            statistic: *statistic,
                        })
                    }
                    MetricValue::AggregatedHistogram { .. } => None,
                    MetricValue::AggregatedSummary { .. } => None,
                    MetricValue::Sketch { .. } => None,
                    MetricValue::Set { .. } => {
                        let mut values = BTreeSet::new();
                        values.insert(self.suffix.clone());
                        Some(MetricValue::Set { values })
                    }
                };
                if let Some(increment) = increment {
                    assert!(metric.add(&MetricData {
                        kind: metric.kind(),
                        timestamp: metric.timestamp(),
                        value: increment,
                    }));
                }
            }
            Event::Trace(trace) => {
                let mut v = trace
                    .get(vector::config::log_schema().message_key())
                    .unwrap()
                    .to_string_lossy();
                v.push_str(&self.suffix);
                trace.insert(vector::config::log_schema().message_key(), Value::from(v));
            }
        };
        output.push(event);
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MockTransformConfig {
    suffix: String,
    increase: f64,
}

impl MockTransformConfig {
    pub fn new(suffix: String, increase: f64) -> Self {
        Self { suffix, increase }
    }
}

#[async_trait]
#[typetag::serde(name = "mock")]
impl TransformConfig for MockTransformConfig {
    async fn build(&self, _globals: &TransformContext) -> Result<Transform, vector::Error> {
        Ok(Transform::function(MockTransform {
            suffix: self.suffix.clone(),
            increase: self.increase,
        }))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn transform_type(&self) -> &'static str {
        "mock"
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MockSinkConfig {
    #[serde(skip)]
    sink: Mode,
    #[serde(skip)]
    healthy: bool,
    // something for serde to use, so we can trigger rebuilds
    data: Option<String>,
}

#[derive(Debug, Clone)]
enum Mode {
    Normal(SourceSender),
    Dead,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Dead
    }
}

impl MockSinkConfig {
    pub fn new(sink: SourceSender, healthy: bool) -> Self {
        Self {
            sink: Mode::Normal(sink),
            healthy,
            data: None,
        }
    }

    pub fn new_dead(healthy: bool) -> Self {
        Self {
            sink: Mode::Dead,
            healthy,
            data: None,
        }
    }

    pub fn new_with_data(sink: SourceSender, healthy: bool, data: &str) -> Self {
        Self {
            sink: Mode::Normal(sink),
            healthy,
            data: Some(data.into()),
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("unhealthy"))]
    Unhealthy,
}

#[async_trait]
#[typetag::serialize(name = "mock")]
impl SinkConfig for MockSinkConfig {
    async fn build(&self, cx: SinkContext) -> Result<(VectorSink, Healthcheck), vector::Error> {
        // If this sink is set to not be healthy, just send the healthcheck error immediately over
        // the oneshot.. otherwise, pass the sender to the sink so it can send it only once it has
        // started running, so that tests can request the topology be healthy before proceeding.
        let (tx, rx) = oneshot::channel();

        let health_tx = if self.healthy {
            Some(tx)
        } else {
            let _ = tx.send(Err(HealthcheckError::Unhealthy.into()));
            None
        };

        let sink = MockSink {
            acker: cx.acker(),
            sink: self.sink.clone(),
            health_tx,
        };

        let healthcheck = async move { rx.await.unwrap() };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck.boxed()))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn sink_type(&self) -> &'static str {
        "mock"
    }

    fn typetag_deserialize(&self) {
        unimplemented!("not intended for use in real configs")
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        None
    }
}

struct MockSink {
    acker: Acker,
    sink: Mode,
    health_tx: Option<oneshot::Sender<vector::Result<()>>>,
}

#[async_trait]
impl StreamSink<Event> for MockSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        match self.sink {
            Mode::Normal(mut sink) => {
                if let Some(tx) = self.health_tx.take() {
                    let _ = tx.send(Ok(()));
                }

                // We have an inner sink, so forward the input normally
                while let Some(event) = input.next().await {
                    if let Err(error) = sink.send_event(event).await {
                        error!(message = "Ingesting an event failed at mock sink.", %error);
                    }

                    self.acker.ack(1);
                }
            }
            Mode::Dead => {
                // Simulate a dead sink and never poll the input
                futures::future::pending::<()>().await;
            }
        }

        Ok(())
    }
}
