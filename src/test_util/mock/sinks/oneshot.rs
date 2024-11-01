use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
use tokio::sync::oneshot::Sender;
use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    event::EventArray,
    sink::{StreamSink, VectorSink},
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::Healthcheck,
};

/// Configurable for the `test_oneshot` sink.
#[configurable_component(sink("test_oneshot", "Test (oneshot)."))]
#[derive(Clone, Debug, Default)]
pub struct OneshotSinkConfig {
    #[serde(skip)]
    tx: Arc<Mutex<Option<Sender<EventArray>>>>,
}

impl_generate_config_from_default!(OneshotSinkConfig);

impl OneshotSinkConfig {
    pub fn new(tx: Sender<EventArray>) -> Self {
        Self {
            tx: Arc::new(Mutex::new(Some(tx))),
        }
    }
}

#[async_trait]
#[typetag::serde(name = "test_oneshot")]
impl SinkConfig for OneshotSinkConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }

    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let tx = {
            let mut guard = self.tx.lock().expect("who cares if the lock is poisoned");
            guard.take()
        };
        let sink = Box::new(OneshotSink { tx });

        let healthcheck = Box::pin(async { Ok(()) });

        Ok((VectorSink::Stream(sink), healthcheck))
    }
}

struct OneshotSink {
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
        _ = tx.send(events);

        Ok(())
    }
}
