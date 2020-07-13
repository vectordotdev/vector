//! A mock state.

use async_trait::async_trait;
use futures::{
    channel::mpsc::{Receiver, Sender},
    future::BoxFuture,
    SinkExt, StreamExt,
};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::ObjectMeta, Metadata};

/// The kind of item-scoped operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// Item added.
    Add,
    /// Item updated.
    Update,
    /// Item deleted.
    Delete,
}

/// An event that's send to the test scenario driver for operations flow.
#[derive(Debug)]
pub enum ScenarioEvent<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    /// An item-scoped operation.
    Item(T, OpKind),
    /// Resync operation.
    Resync,
    /// Maintenance is performed.
    Maintenance,
}

impl<T> ScenarioEvent<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    /// Unwraps the item and operation kind, or panics if the event is not
    /// an item event.
    pub fn unwrap_item(self) -> (T, OpKind) {
        match self {
            ScenarioEvent::Item(val, op) => (val, op),
            _ => panic!("unwrap_item on non-item event"),
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
#[derive(Debug)]
pub struct Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    events_tx: Sender<ScenarioEvent<T>>,
    actions_rx: Receiver<()>,
    maintenance_request: Option<(Sender<()>, Receiver<()>)>,
}

impl<T> Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    /// Create a new mock writer.
    /// Takes:
    /// - `events_tx` - a message is sent here at the beginning of the
    ///    operation.
    /// - `actions_rx` - a message is read from here before the operation
    ///    returns.
    pub fn new(events_tx: Sender<ScenarioEvent<T>>, actions_rx: Receiver<()>) -> Self {
        Self {
            events_tx,
            actions_rx,
            maintenance_request: None,
        }
    }

    /// Create a new mock writer (with maintenance flow).
    /// Takes:
    /// - `events_tx` - a message is sent here at the beginning of the
    ///    operation.
    /// - `actions_rx` - a message is read from here before the operation
    ///    returns;
    /// - `maintenance_request_events_tx` - a message is sent here at the
    ///    beginning of the maintenance request;
    /// - `maintenance_request_events_tx` - a message is read from here before
    ///    the maintenance request returns.
    pub fn new_with_maintenance(
        events_tx: Sender<ScenarioEvent<T>>,
        actions_rx: Receiver<()>,
        maintenance_request_events_tx: Sender<()>,
        maintenance_request_actions_rx: Receiver<()>,
    ) -> Self {
        Self {
            events_tx,
            actions_rx,
            maintenance_request: Some((
                maintenance_request_events_tx,
                maintenance_request_actions_rx,
            )),
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
            .send(ScenarioEvent::Item(item, OpKind::Add))
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }

    async fn update(&mut self, item: Self::Item) {
        self.events_tx
            .send(ScenarioEvent::Item(item, OpKind::Update))
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }

    async fn delete(&mut self, item: Self::Item) {
        self.events_tx
            .send(ScenarioEvent::Item(item, OpKind::Delete))
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }

    async fn resync(&mut self) {
        self.events_tx.send(ScenarioEvent::Resync).await.unwrap();
        self.actions_rx.next().await.unwrap();
    }
}

#[async_trait]
impl<T> super::MaintainedWrite for Writer<T>
where
    T: Metadata<Ty = ObjectMeta> + Send,
{
    fn maintenance_request(&mut self) -> Option<BoxFuture<'_, ()>> {
        if let Some((ref mut events_tx, ref mut actions_rx)) = self.maintenance_request {
            Some(Box::pin(async move {
                events_tx.send(()).await.unwrap();
                actions_rx.next().await.unwrap();
            }))
        } else {
            None
        }
    }

    async fn perform_maintenance(&mut self) {
        self.events_tx
            .send(ScenarioEvent::Maintenance)
            .await
            .unwrap();
        self.actions_rx.next().await.unwrap();
    }
}
