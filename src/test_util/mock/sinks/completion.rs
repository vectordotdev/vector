use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::{FutureExt, StreamExt, future, stream::BoxStream};
use tokio::sync::oneshot::Sender;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    configurable::configurable_component,
    event::Event,
    sink::{StreamSink, VectorSink},
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::Healthcheck,
};

/// Configuration for the `test_completion` sink.
#[configurable_component(sink("test_completion", "Test (completion)."))]
#[derive(Clone, Debug, Default)]
pub struct CompletionSinkConfig {
    #[serde(skip)]
    expected: usize,

    #[serde(skip)]
    completion_tx: Arc<Mutex<Option<Sender<bool>>>>,
}

impl_generate_config_from_default!(CompletionSinkConfig);

impl CompletionSinkConfig {
    pub fn new(expected: usize, completion_tx: Sender<bool>) -> Self {
        Self {
            expected,
            completion_tx: Arc::new(Mutex::new(Some(completion_tx))),
        }
    }
}

#[async_trait]
#[typetag::serde(name = "test_completion")]
impl SinkConfig for CompletionSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let completion_tx = self
            .completion_tx
            .lock()
            .expect("completion sink mutex poisoned")
            .take();

        let sink = CompletionSink {
            remaining: self.expected,
            completion_tx,
        };
        let healthcheck = future::ready(Ok(())).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &AcknowledgementsConfig::DEFAULT
    }
}

struct CompletionSink {
    remaining: usize,
    completion_tx: Option<Sender<bool>>,
}

#[async_trait]
impl StreamSink<Event> for CompletionSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            drop(event);

            if self.remaining > 0 {
                self.remaining -= 1;
                if self.remaining == 0
                    && let Some(tx) = self.completion_tx.take()
                {
                    let _ = tx.send(true);
                }
            }
        }

        if let Some(tx) = self.completion_tx.take() {
            let _ = tx.send(self.remaining == 0);
        }

        Ok(())
    }
}
