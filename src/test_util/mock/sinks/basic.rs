use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::{FutureExt, StreamExt, stream::BoxStream};
use snafu::Snafu;
use tokio::sync::oneshot;
use vector_lib::{
    buffers::BufferUsageObserver,
    config::{AcknowledgementsConfig, Input},
    configurable::configurable_component,
    event::Event,
    finalization::Finalizable,
    sink::{StreamSink, VectorSink},
};

use crate::{
    SourceSender,
    config::{SinkConfig, SinkContext},
    sinks::Healthcheck,
};

/// Configuration for the `test_basic` sink.
#[configurable_component(sink("test_basic", "Test (basic)."))]
#[derive(Clone, Debug, Default)]
pub struct BasicSinkConfig {
    #[serde(skip)]
    sink: Mode,

    #[serde(skip)]
    healthy: bool,

    /// Dummy field used for generating unique configurations to trigger reloads.
    data: Option<String>,

    #[serde(skip)]
    requires_buffer_observation: bool,

    #[serde(skip)]
    captured_buffer_usage_observers: Option<Arc<Mutex<Vec<Option<BufferUsageObserver>>>>>,
}

impl_generate_config_from_default!(BasicSinkConfig);

#[derive(Debug, Default, Clone)]
#[allow(clippy::large_enum_variant)]
enum Mode {
    Normal(SourceSender),
    #[default]
    Dead,
}

impl BasicSinkConfig {
    pub fn new(sink: SourceSender, healthy: bool) -> Self {
        Self {
            sink: Mode::Normal(sink),
            healthy,
            data: None,
            requires_buffer_observation: false,
            captured_buffer_usage_observers: None,
        }
    }

    pub fn new_with_data(sink: SourceSender, healthy: bool, data: &str) -> Self {
        Self {
            sink: Mode::Normal(sink),
            healthy,
            data: Some(data.into()),
            requires_buffer_observation: false,
            captured_buffer_usage_observers: None,
        }
    }

    pub fn with_buffer_observation(mut self, requires_buffer_observation: bool) -> Self {
        self.requires_buffer_observation = requires_buffer_observation;
        self
    }

    pub fn with_captured_buffer_usage_observers(
        mut self,
        captured_buffer_usage_observers: Arc<Mutex<Vec<Option<BufferUsageObserver>>>>,
    ) -> Self {
        self.captured_buffer_usage_observers = Some(captured_buffer_usage_observers);
        self
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("unhealthy"))]
    Unhealthy,
}

#[async_trait]
#[typetag::serde(name = "test_basic")]
impl SinkConfig for BasicSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        if let Some(captured_buffer_usage_observers) = &self.captured_buffer_usage_observers {
            captured_buffer_usage_observers
                .lock()
                .unwrap()
                .push(cx.buffer_usage_observer.clone());
        }

        // If this sink is set to not be healthy, just send the healthcheck error immediately over
        // the oneshot.. otherwise, pass the sender to the sink so it can send it only once it has
        // started running, so that tests can request the topology be healthy before proceeding.
        let (tx, rx) = oneshot::channel();

        let health_tx = if self.healthy {
            Some(tx)
        } else {
            _ = tx.send(Err(HealthcheckError::Unhealthy.into()));
            None
        };

        let sink = MockSink {
            sink: self.sink.clone(),
            health_tx,
        };

        let healthcheck = async move { rx.await.unwrap() };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck.boxed()))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn requires_buffer_observation(&self) -> bool {
        self.requires_buffer_observation
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

struct MockSink {
    sink: Mode,
    health_tx: Option<oneshot::Sender<crate::Result<()>>>,
}

#[async_trait]
impl StreamSink<Event> for MockSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        match self.sink {
            Mode::Normal(mut sink) => {
                if let Some(tx) = self.health_tx.take() {
                    _ = tx.send(Ok(()));
                }

                // We have an inner sink, so forward the input normally
                while let Some(mut event) = input.next().await {
                    let finalizers = event.take_finalizers();
                    if let Err(error) = sink.send_event(event).await {
                        error!(message = "Ingesting an event failed at mock sink.", %error);
                    }
                    drop(finalizers);
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
