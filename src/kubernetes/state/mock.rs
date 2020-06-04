//! A mock state.

#![cfg(test)]

use async_trait::async_trait;
use futures::{
    channel::mpsc::{Receiver, Sender},
    SinkExt, StreamExt,
};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};

/// The kind of operation.
#[derive(Debug, PartialEq, Eq)]
pub enum OpKind {
    /// Item added.
    Add,
    /// Item updated.
    Update,
    /// Item deleted.
    Delete,
}

/// An event that's send to the test scenario driver.
pub enum ScenarioEvent<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    Op(T, OpKind),
    Resync,
}

impl<T> ScenarioEvent<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    pub fn unwrap_op(self) -> (T, OpKind) {
        match self {
            ScenarioEvent::Op(val, op) => (val, op),
            ScenarioEvent::Resync => panic!("unwrap_op on resync"),
        }
    }
}

/// Mock writer.
///
/// Uses channels to communicate with the test scenario driver.
///
/// When the call is made on the mock - sends an event to the `events_tx` and
/// waits for at action to conduct in response to the event `actions_rx`.
///
/// Note: the only action available in the [`super::Write`] is to just continue
/// and return.
pub struct Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    events_tx: Sender<ScenarioEvent<T>>,
    actions_rx: Receiver<()>,
}

impl<T> Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    /// Create a new mock writer.
    /// Takes:
    /// - `events_tx`, to which it sends the events when the mock action is
    ///    called;
    /// - `actions_rx`, that is read a message from before the mock action
    ///    returns.
    pub fn new(events_tx: Sender<ScenarioEvent<T>>, actions_rx: Receiver<()>) -> Self {
        Self {
            events_tx,
            actions_rx,
        }
    }
}

#[async_trait]
impl<T> super::Write for Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    type Item = T;

    async fn add(&mut self, item: Self::Item) {
        self.events_tx
            .send(ScenarioEvent::Op(item, OpKind::Add))
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }

    async fn update(&mut self, item: Self::Item) {
        self.events_tx
            .send(ScenarioEvent::Op(item, OpKind::Update))
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }

    async fn delete(&mut self, item: Self::Item) {
        self.events_tx
            .send(ScenarioEvent::Op(item, OpKind::Delete))
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }

    async fn resync(&mut self) {
        self.events_tx.send(ScenarioEvent::Resync).await.unwrap();
        self.actions_rx.next().await.unwrap();
    }
}
