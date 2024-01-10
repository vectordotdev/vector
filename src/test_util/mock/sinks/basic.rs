use async_trait::async_trait;
use futures_util::{stream::BoxStream, FutureExt, StreamExt};
use snafu::Snafu;
use tokio::sync::oneshot;
use vector_lib::configurable::configurable_component;
use vector_lib::finalization::Finalizable;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    event::Event,
    sink::{StreamSink, VectorSink},
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::Healthcheck,
    SourceSender,
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
    pub const fn new(sink: SourceSender, healthy: bool) -> Self {
        Self {
            sink: Mode::Normal(sink),
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
#[typetag::serde(name = "test_basic")]
impl SinkConfig for BasicSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
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
