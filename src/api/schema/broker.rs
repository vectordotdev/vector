use futures::{
    channel::mpsc::{self, Receiver, Sender},
    task::{Context, Poll},
    Stream, StreamExt,
};
use once_cell::sync::Lazy;
use serde::export::PhantomData;
use slab::Slab;
use std::{pin::Pin, sync::Mutex};

/// Current subscriptions. This is static so that 'global' changes can be published to any
/// connecting subscriber. Subscribers fall out of scope once the subscription is terminated
static SUBSCRIBERS: Lazy<Mutex<anymap::Map<dyn anymap::any::Any + Send>>> =
    Lazy::new(|| Mutex::new(anymap::Map::new()));

struct Senders<T>(Slab<Sender<T>>);
struct BrokerStream<T: Sync + Send + Clone + 'static>(usize, Receiver<T>);

fn with_senders<T, F, R>(f: F) -> R
where
    T: Sync + Send + Clone + 'static,
    F: FnOnce(&mut Senders<T>) -> R,
{
    let mut map = SUBSCRIBERS.lock().unwrap();
    let senders = map
        .entry()
        .or_insert_with(|| Box::new(Senders::<T>(Default::default())));
    f(senders)
}

impl<T: Sync + Send + Clone + 'static> Drop for BrokerStream<T> {
    fn drop(&mut self) {
        with_senders::<T, _, _>(|senders| senders.0.remove(self.0));
    }
}

impl<T: Sync + Send + Clone + 'static> Stream for BrokerStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.1.poll_next_unpin(cx)
    }
}

/// Broker contains the pub/sub methods for publishing 'events' to connecting subscribers
pub struct Broker<T>(PhantomData<T>);

impl<T: Sync + Send + Clone + 'static> Broker<T> {
    /// Publish a message that all subscription streams can receive
    pub fn publish(msg: T) {
        with_senders::<T, _, _>(|senders| {
            for (_, sender) in senders.0.iter_mut() {
                sender.start_send(msg.clone()).ok();
            }
        });
    }

    /// Subscribe to the message of the specified type and returns a `Stream<T>`, for
    /// use with GraphQL subscriptions
    pub fn subscribe() -> impl Stream<Item = T> {
        with_senders::<T, _, _>(|senders| {
            let (tx, rx) = mpsc::channel(20);
            let id = senders.0.insert(tx);
            BrokerStream(id, rx)
        })
    }
}
