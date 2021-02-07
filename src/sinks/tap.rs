use super::util::StreamSink;
use crate::{
    buffers::Acker,
    config::{DataType, SinkContext, SinkOuter},
};
use crate::{
    config::SinkConfig,
    event::{Event, LogEvent},
};
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;

type Sender = broadcast::Sender<LogEvent>;
type Receiver = broadcast::Receiver<LogEvent>;

#[derive(Debug)]
pub struct TapSink {
    tx: Sender,
    acker: Acker,
}

/// Tap sink is used to 'tap' into events received by upstream components, and broadcast
/// them to subscribers. Typically, this is used to expose events to the API, but is general
/// purpose enough that it could technically be used with other mechanisms. This sink is
/// not added to inventory; it's not intended to be user configurable.
impl TapSink {
    pub fn new(tx: Sender, acker: Acker) -> Self {
        Self { tx, acker }
    }
}

#[async_trait::async_trait]
impl StreamSink for TapSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            if let Event::Log(event) = event {
                // This can suffer from TOC/TOU, but the risk is minimal as the purpose
                // here is solely to reduce expense.
                if self.tx.receiver_count() > 0 {
                    let _ = self.tx.send(event.clone());
                }
            }
            self.acker.ack(1);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
/// Since 'tap' isn't user configurable, we're using TapConfig as middleware to satisfy the
/// `SinkConfig` trait, and pass along the relaying Sender to the eventual sink. This is wrapped
/// in a RwLock for interior mutability, since `SinkConfig.build` requires an immutability borrow.
/// The Option<Sender> wrapper is used to replace the value with `None` when taken.
pub struct TapConfig {
    #[serde(skip_deserializing, skip_serializing, default = "default_locked_tx")]
    locked_tx: RwLock<Option<Sender>>,
}

fn default_locked_tx() -> RwLock<Option<Sender>> {
    RwLock::new(None)
}

impl TapConfig {
    fn new(tx: Sender) -> Self {
        Self {
            locked_tx: RwLock::new(Some(tx)),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "tap")]
impl SinkConfig for TapConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let mut lock = self.locked_tx.write();

        let sink = TapSink::new(lock.take().expect("Expected TapConfig tx"), cx.acker);
        let healthcheck = future::ok(()).boxed();

        Ok((super::VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "tap"
    }
}

/// This controller represents the public interface to subscribe to underlying `LogEvent`s
/// specific to a given component, identified by its name.
pub struct TapController {
    senders: RwLock<HashMap<String, Sender>>,
}

impl TapController {
    pub fn new() -> Self {
        Self {
            senders: RwLock::new(HashMap::new()),
        }
    }

    /// Returns a new `SinkOuter`, which can be inserted into sink config, mapping a component's
    /// input name with a specific broadcast channel
    pub fn make_sink(&self, component_name: &str) -> SinkOuter {
        let mut lock = self.senders.write();
        let tx = lock.entry(component_name.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(100);
            tx
        });

        // A `SinkConfig` is required to be provided to SinkOuter, so we create one here,
        // passing a tx clone which will in turn be 'taken' by the end sink.
        let tap_config = TapConfig::new(tx.clone());

        SinkOuter::new(vec![component_name.to_string()], Box::new(tap_config))
    }

    /// Subscribe to `LogEvent`s received against a specific component name. Any additional
    /// filtering should be done by the downstream.
    pub fn subscribe(&self, component_name: &str) -> Option<Receiver> {
        self.senders
            .read()
            .get(component_name)
            .map(|tx| tx.subscribe())
    }
}

impl Default for TapController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::random_events_with_stream;
    use tokio::stream::StreamExt;

    #[tokio::test]
    async fn tap() {
        let (tx, mut rx) = broadcast::channel(100);
        let mut sink = TapSink::new(tx, Acker::Null);
        let count = 10;

        // Assert that we received events out of the other side
        let handle = tokio::spawn(async move {
            for _ in 0..count {
                let event = rx.next().await.unwrap().unwrap();
                let fields = event.all_fields().collect::<Vec<_>>();

                assert_eq!(fields[0].0, "message");
                assert_eq!(fields[1].0, "timestamp");
            }
        });

        let (_input_lines, events) = random_events_with_stream(100, count);
        let _ = sink.run(Box::pin(events)).await.unwrap();
        let _ = handle.await;
    }
}
