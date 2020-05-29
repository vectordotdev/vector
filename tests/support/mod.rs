// Using a shared mod like this is probably not the best idea, since we have to
// disable the `dead_code` lint, as we don't need all of the helpers from here
// all over the place.
#![allow(dead_code)]

use futures01::{
    future,
    sink::Sink,
    stream,
    sync::mpsc::{Receiver, Sender},
    Async, Future, Stream,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tracing::{error, info};
use vector::event::{self, metric::MetricValue, Event, Value};
use vector::shutdown::ShutdownSignal;
use vector::sinks::{util::StreamSink, Healthcheck, RouterSink};
use vector::sources::Source;
use vector::topology::config::{
    DataType, GlobalOptions, SinkConfig, SinkContext, SourceConfig, TransformConfig,
    TransformContext,
};
use vector::transforms::Transform;

pub fn sink(channel_size: usize) -> (Receiver<Event>, MockSinkConfig<Sender<Event>>) {
    let (tx, rx) = futures01::sync::mpsc::channel(channel_size);
    let sink = MockSinkConfig::new(tx, true);
    (rx, sink)
}

pub fn sink_failing_healthcheck(
    channel_size: usize,
) -> (Receiver<Event>, MockSinkConfig<Sender<Event>>) {
    let (tx, rx) = futures01::sync::mpsc::channel(channel_size);
    let sink = MockSinkConfig::new(tx, false);
    (rx, sink)
}

pub fn sink_dead() -> MockSinkConfig<DeadSink<Event>> {
    MockSinkConfig::new(DeadSink::new(), false)
}

pub fn source() -> (Sender<Event>, MockSourceConfig) {
    let (tx, rx) = futures01::sync::mpsc::channel(0);
    let source = MockSourceConfig::new(rx);
    (tx, source)
}

pub fn source_with_event_counter() -> (Sender<Event>, MockSourceConfig, Arc<AtomicUsize>) {
    let event_counter = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = futures01::sync::mpsc::channel(0);
    let source = MockSourceConfig::new_with_event_counter(rx, event_counter.clone());
    (tx, source, event_counter)
}

pub fn transform(suffix: &str, increase: f64) -> MockTransformConfig {
    MockTransformConfig::new(suffix.to_owned(), increase)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MockSourceConfig {
    #[serde(skip)]
    receiver: Arc<Mutex<Option<Receiver<Event>>>>,
    #[serde(skip)]
    event_counter: Option<Arc<AtomicUsize>>,
    #[serde(skip)]
    data_type: Option<DataType>,
}

impl MockSourceConfig {
    pub fn new(receiver: Receiver<Event>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::Any),
        }
    }

    pub fn new_with_event_counter(
        receiver: Receiver<Event>,
        event_counter: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: Some(event_counter),
            data_type: Some(DataType::Any),
        }
    }

    pub fn set_data_type(&mut self, data_type: DataType) {
        self.data_type = Some(data_type)
    }
}

#[typetag::serde(name = "mock")]
impl SourceConfig for MockSourceConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Sender<Event>,
    ) -> Result<Source, vector::Error> {
        let wrapped = self.receiver.clone();
        let event_counter = self.event_counter.clone();
        let mut recv = wrapped.lock().unwrap().take().unwrap();
        let mut shutdown = Some(shutdown);
        let mut token = None;
        let source = future::lazy(move || {
            stream::poll_fn(move || {
                if let Some(until) = shutdown.as_mut() {
                    match until.poll() {
                        Ok(Async::Ready(res)) => {
                            token = Some(res);
                            shutdown.take();
                            recv.close();
                        }
                        Err(_) => {
                            shutdown.take();
                        }
                        Ok(Async::NotReady) => {}
                    }
                }
                recv.poll()
            })
            .map(move |x| {
                if let Some(counter) = &event_counter {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
                x
            })
            .forward(out.sink_map_err(|e| error!("Error sending in sink {}", e)))
            .map(|_| info!("finished sending"))
        });
        Ok(Box::new(source))
    }

    fn output_type(&self) -> DataType {
        self.data_type.clone().unwrap()
    }

    fn source_type(&self) -> &'static str {
        "mock"
    }
}

pub struct MockTransform {
    suffix: String,
    increase: f64,
}

impl Transform for MockTransform {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        match &mut event {
            Event::Log(log) => {
                let mut v = log
                    .get(&event::log_schema().message_key())
                    .unwrap()
                    .to_string_lossy();
                v.push_str(&self.suffix);
                log.insert(event::log_schema().message_key().clone(), Value::from(v));
            }
            Event::Metric(metric) => match metric.value {
                MetricValue::Counter { ref mut value } => {
                    *value += self.increase;
                }
                MetricValue::Distribution {
                    ref mut values,
                    ref mut sample_rates,
                } => {
                    values.push(self.increase);
                    sample_rates.push(1);
                }
                MetricValue::AggregatedHistogram {
                    ref mut count,
                    ref mut sum,
                    ..
                } => {
                    *count += 1;
                    *sum += self.increase;
                }
                MetricValue::AggregatedSummary {
                    ref mut count,
                    ref mut sum,
                    ..
                } => {
                    *count += 1;
                    *sum += self.increase;
                }
                MetricValue::Gauge { ref mut value, .. } => {
                    *value += self.increase;
                }
                MetricValue::Set { ref mut values, .. } => {
                    values.insert(self.suffix.clone());
                }
            },
        };
        Some(event)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MockTransformConfig {
    suffix: String,
    increase: f64,
}

impl MockTransformConfig {
    pub fn new(suffix: String, increase: f64) -> Self {
        Self { suffix, increase }
    }
}

#[typetag::serde(name = "mock")]
impl TransformConfig for MockTransformConfig {
    fn build(&self, _cx: TransformContext) -> Result<Box<dyn Transform>, vector::Error> {
        Ok(Box::new(MockTransform {
            suffix: self.suffix.clone(),
            increase: self.increase,
        }))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "mock"
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MockSinkConfig<T>
where
    T: Sink<SinkItem = Event> + std::fmt::Debug + Clone + Send + 'static,
    <T as Sink>::SinkError: std::fmt::Debug,
{
    #[serde(skip)]
    sink: Option<T>,
    #[serde(skip)]
    healthy: bool,
}

impl<T> MockSinkConfig<T>
where
    T: Sink<SinkItem = Event> + std::fmt::Debug + Clone + Send + 'static,
    <T as Sink>::SinkError: std::fmt::Debug,
{
    pub fn new(sink: T, healthy: bool) -> Self {
        Self {
            sink: Some(sink),
            healthy,
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("unhealthy"))]
    Unhealthy,
}

#[typetag::serialize(name = "mock")]
impl<T> SinkConfig for MockSinkConfig<T>
where
    T: Sink<SinkItem = Event> + std::fmt::Debug + Clone + Send + 'static,
    <T as Sink>::SinkError: std::fmt::Debug,
{
    fn build(&self, cx: SinkContext) -> Result<(RouterSink, Healthcheck), vector::Error> {
        let sink = self.sink.clone().unwrap();
        let sink = sink.sink_map_err(|error| {
            error!(message = "Ingesting an event failed at mock sink", ?error)
        });
        let sink = StreamSink::new(sink, cx.acker());
        let healthcheck = match self.healthy {
            true => future::ok(()),
            false => future::err(HealthcheckError::Unhealthy.into()),
        };
        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "mock"
    }

    fn typetag_deserialize(&self) {
        unimplemented!("not intended for use in real configs")
    }
}

/// Represents a sink that's never ready.
/// Useful to simulate an upstream sink server that is down.
#[derive(Debug, Clone)]
pub struct DeadSink<T>(std::marker::PhantomData<T>);

impl<T> DeadSink<T> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T> Sink for DeadSink<T> {
    type SinkItem = T;
    type SinkError = &'static str;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures01::StartSend<Self::SinkItem, Self::SinkError> {
        Ok(futures01::AsyncSink::NotReady(item))
    }

    fn poll_complete(&mut self) -> futures01::Poll<(), Self::SinkError> {
        Ok(futures01::Async::Ready(()))
    }
}
