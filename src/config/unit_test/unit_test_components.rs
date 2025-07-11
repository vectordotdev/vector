use std::sync::Arc;

use futures::{stream, Sink, Stream};
use futures_util::{future, stream::BoxStream, FutureExt, StreamExt};
use tokio::sync::{oneshot, Mutex};
use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::{DataType, Input, LogNamespace},
    event::Event,
    schema,
    sink::{StreamSink, VectorSink},
};

use crate::{
    conditions::Condition,
    config::{
        AcknowledgementsConfig, SinkConfig, SinkContext, SourceConfig, SourceContext, SourceOutput,
    },
    sinks::Healthcheck,
    sources,
};

/// Configuration for the `unit_test` source.
#[configurable_component(source("unit_test", "Unit test."))]
#[derive(Clone, Debug, Default)]
pub struct UnitTestSourceConfig {
    /// List of events sent from this source as part of the test.
    #[serde(skip)]
    pub events: Vec<Event>,
}

impl_generate_config_from_default!(UnitTestSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SourceConfig for UnitTestSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let events = self.events.clone().into_iter();

        Ok(Box::pin(async move {
            let mut out = cx.out;
            let _shutdown = cx.shutdown;
            out.send_batch(events).await.map_err(|_| ())?;
            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(
            DataType::all_bits(),
            schema::Definition::default_legacy_namespace(),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

/// Configuration for the `unit_test_stream` source.
#[configurable_component(source("unit_test_stream", "Unit test stream."))]
#[derive(Clone)]
pub struct UnitTestStreamSourceConfig {
    #[serde(skip)]
    stream: Arc<Mutex<Option<stream::BoxStream<'static, Event>>>>,
}

impl_generate_config_from_default!(UnitTestStreamSourceConfig);

impl UnitTestStreamSourceConfig {
    pub fn new(stream: impl Stream<Item = Event> + Send + 'static) -> Self {
        Self {
            stream: Arc::new(Mutex::new(Some(stream.boxed()))),
        }
    }
}

impl Default for UnitTestStreamSourceConfig {
    fn default() -> Self {
        Self::new(stream::empty().boxed())
    }
}

impl std::fmt::Debug for UnitTestStreamSourceConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UnitTestStreamSourceConfig")
            .finish()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test_stream")]
impl SourceConfig for UnitTestStreamSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let stream = self.stream.lock().await.take().unwrap();
        Ok(Box::pin(async move {
            let mut out = cx.out;
            let _shutdown = cx.shutdown;
            out.send_event_stream(stream).await.map_err(|_| ())?;
            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(
            DataType::all_bits(),
            schema::Definition::default_legacy_namespace(),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone, Default)]
pub enum UnitTestSinkCheck {
    /// Check all events that are received against the list of conditions.
    Checks(Vec<Vec<Condition>>),

    /// Check that no events were received.
    NoOutputs,

    /// Do nothing.
    #[default]
    NoOp,
}

#[derive(Debug)]
pub struct UnitTestSinkResult {
    pub test_name: String,
    pub test_errors: Vec<String>,
}

/// Configuration for the `unit_test` sink.
#[configurable_component(sink("unit_test", "Unit test."))]
#[derive(Clone, Default, Derivative)]
#[derivative(Debug)]
pub struct UnitTestSinkConfig {
    /// Name of the test that this sink is being used for.
    pub test_name: String,

    /// List of names of the transform/branch associated with this sink.
    pub transform_ids: Vec<String>,

    /// Sender side of the test result channel.
    #[serde(skip)]
    pub result_tx: Arc<Mutex<Option<oneshot::Sender<UnitTestSinkResult>>>>,

    /// Predicate applied to each event that reaches the sink.
    #[serde(skip)]
    #[derivative(Debug = "ignore")]
    pub check: UnitTestSinkCheck,
}

impl_generate_config_from_default!(UnitTestSinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test")]
impl SinkConfig for UnitTestSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tx = self.result_tx.lock().await.take();
        let sink = UnitTestSink {
            test_name: self.test_name.clone(),
            transform_ids: self.transform_ids.clone(),
            result_tx: tx,
            check: self.check.clone(),
        };
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

pub struct UnitTestSink {
    pub test_name: String,
    pub transform_ids: Vec<String>,
    // None for NoOp test sinks
    pub result_tx: Option<oneshot::Sender<UnitTestSinkResult>>,
    pub check: UnitTestSinkCheck,
}

#[async_trait::async_trait]
impl StreamSink<Event> for UnitTestSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut output_events = Vec::new();
        let mut result = UnitTestSinkResult {
            test_name: self.test_name,
            test_errors: Vec::new(),
        };

        while let Some(event) = input.next().await {
            output_events.push(event);
        }

        match self.check {
            UnitTestSinkCheck::Checks(checks) => {
                if output_events.is_empty() {
                    result
                        .test_errors
                        .push(format!("checks for transforms {:?} failed: no events received. Topology may be disconnected or transform is missing inputs.", self.transform_ids));
                } else {
                    for (i, check) in checks.iter().enumerate() {
                        let mut check_errors = Vec::new();
                        for (j, condition) in check.iter().enumerate() {
                            let mut condition_errors = Vec::new();
                            for event in output_events.iter() {
                                match condition.check_with_context(event.clone()).0 {
                                    Ok(_) => {
                                        condition_errors.clear();
                                        break;
                                    }
                                    Err(error) => {
                                        condition_errors
                                            .push(format!("  condition[{}]: {}", j, error));
                                    }
                                }
                            }
                            check_errors.extend(condition_errors);
                        }
                        // If there are errors, add a preamble to the output
                        if !check_errors.is_empty() {
                            check_errors.insert(
                                0,
                                format!(
                                    "check[{}] for transforms {:?} failed conditions:",
                                    i, self.transform_ids
                                ),
                            );
                        }

                        result.test_errors.extend(check_errors);
                    }

                    // If there are errors, add a summary of events received
                    if !result.test_errors.is_empty() {
                        result.test_errors.push(format!(
                            "output payloads from {:?} (events encoded as JSON):\n  {}",
                            self.transform_ids,
                            events_to_string(&output_events)
                        ));
                    }
                }
            }
            UnitTestSinkCheck::NoOutputs => {
                if !output_events.is_empty() {
                    result.test_errors.push(format!(
                        "check for transforms {:?} failed: expected no outputs",
                        self.transform_ids
                    ));
                }
            }
            UnitTestSinkCheck::NoOp => {}
        }

        if let Some(tx) = self.result_tx {
            if tx.send(result).is_err() {
                error!(message = "Sending unit test results failed in unit test sink.");
            }
        }
        Ok(())
    }
}

/// Configuration for the `unit_test_stream` sink.
#[configurable_component(sink("unit_test_stream", "Unit test stream."))]
#[derive(Clone, Default)]
pub struct UnitTestStreamSinkConfig {
    /// Sink that receives the processed events.
    #[serde(skip)]
    sink: Arc<Mutex<Option<Box<dyn Sink<Event, Error = ()> + Send + Unpin>>>>,
}

impl_generate_config_from_default!(UnitTestStreamSinkConfig);

impl UnitTestStreamSinkConfig {
    pub fn new(sink: impl Sink<Event, Error = ()> + Send + Unpin + 'static) -> Self {
        Self {
            sink: Arc::new(Mutex::new(Some(Box::new(sink)))),
        }
    }
}

impl std::fmt::Debug for UnitTestStreamSinkConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("UnitTestStreamSinkConfig").finish()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "unit_test_stream")]
impl SinkConfig for UnitTestStreamSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = self.sink.lock().await.take().unwrap();
        let healthcheck = future::ok(()).boxed();

        #[allow(deprecated)]
        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

fn events_to_string(events: &[Event]) -> String {
    events
        .iter()
        .map(|event| match event {
            Event::Log(log) => serde_json::to_string(log).unwrap_or_else(|_| "{}".to_string()),
            Event::Metric(metric) => {
                serde_json::to_string(metric).unwrap_or_else(|_| "{}".to_string())
            }
            Event::Trace(trace) => {
                serde_json::to_string(trace).unwrap_or_else(|_| "{}".to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n  ")
}
