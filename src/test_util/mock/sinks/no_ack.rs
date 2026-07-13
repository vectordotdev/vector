use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::{FutureExt, StreamExt, stream::BoxStream};
use tokio::sync::oneshot;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    configurable::configurable_component,
    event::Event,
    finalization::{EventFinalizers, Finalizable},
    sink::{StreamSink, VectorSink},
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::Healthcheck,
};

/// Configuration for the `test_no_ack` sink.
///
/// This sink receives events but holds their finalizers indefinitely, preventing
/// the batch from being acknowledged. Useful for testing that a blocked or
/// non-acknowledging sink correctly prevents ack delivery.
#[configurable_component(sink("test_no_ack", "Test (no ack)."))]
#[derive(Clone, Debug, Default)]
pub struct NoAckSinkConfig {
    #[serde(skip)]
    healthy: bool,

    #[serde(skip)]
    acknowledgements: AcknowledgementsConfig,

    /// Shared storage for held finalizers, so the test can inspect or drop them.
    #[serde(skip)]
    held_finalizers: Arc<Mutex<Vec<EventFinalizers>>>,

    /// Notifier sent after the first event is received.
    #[serde(skip)]
    event_received_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl_generate_config_from_default!(NoAckSinkConfig);

impl NoAckSinkConfig {
    /// Create a new no-ack sink.
    ///
    /// Returns the config, a receiver that fires when the first event is received,
    /// and the shared held-finalizers storage.
    pub fn new(
        acks: AcknowledgementsConfig,
    ) -> (
        Self,
        oneshot::Receiver<()>,
        Arc<Mutex<Vec<EventFinalizers>>>,
    ) {
        let held_finalizers = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = oneshot::channel();
        let config = Self {
            healthy: true,
            acknowledgements: acks,
            held_finalizers: Arc::clone(&held_finalizers),
            event_received_tx: Arc::new(Mutex::new(Some(tx))),
        };
        (config, rx, held_finalizers)
    }
}

#[async_trait]
#[typetag::serde(name = "test_no_ack")]
impl SinkConfig for NoAckSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let (tx, rx) = oneshot::channel();

        let health_tx = if self.healthy {
            Some(tx)
        } else {
            _ = tx.send(Err("unhealthy".into()));
            None
        };

        let sink = NoAckSink {
            health_tx,
            held_finalizers: Arc::clone(&self.held_finalizers),
            event_received_tx: self.event_received_tx.lock().unwrap().take(),
        };

        let healthcheck = async move { rx.await.unwrap() };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck.boxed()))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct NoAckSink {
    health_tx: Option<oneshot::Sender<crate::Result<()>>>,
    held_finalizers: Arc<Mutex<Vec<EventFinalizers>>>,
    event_received_tx: Option<oneshot::Sender<()>>,
}

#[async_trait]
impl StreamSink<Event> for NoAckSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        if let Some(tx) = self.health_tx.take() {
            _ = tx.send(Ok(()));
        }

        while let Some(mut event) = input.next().await {
            // Take the finalizers but DON'T drop them — hold them to prevent ack.
            let finalizers = event.take_finalizers();
            self.held_finalizers.lock().unwrap().push(finalizers);

            // Notify the test that we received an event (only the first time).
            if let Some(tx) = self.event_received_tx.take() {
                _ = tx.send(());
            }
        }

        Ok(())
    }
}
