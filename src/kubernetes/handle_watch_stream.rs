use futures::StreamExt;
use futures_util::Stream;
use kube::{
    runtime::{reflector::store, watcher},
    Resource,
};
use std::{hash::Hash, time::Duration};
use tokio::pin;
use tokio_util::time::DelayQueue;

pub async fn handle_watch_stream<K, W>(
    mut store: store::Writer<K>,
    stream: W,
    delay_deletion: Duration,
) where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
    W: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    pin!(stream);
    let mut delay_queue = DelayQueue::default();
    loop {
        tokio::select! {
            Some(Ok(event)) = stream.next() => {
                match event {
                    watcher::Event::Applied(_) => {
                        // Immediately apply event
                        store.apply_watcher_event(&event);
                    }
                    watcher::Event::Deleted(_) => {
                        // Delay reconciling any `Deleted` events
                        delay_queue.insert(event.to_owned(), delay_deletion);
                    }
                    watcher::Event::Restarted(_) => {
                        // Clear all delayed events when the cache is refreshed
                        delay_queue.clear();
                        store.apply_watcher_event(&event);
                    }
                }
            }
            Some(Ok(event)) = delay_queue.next() => {
                store.apply_watcher_event(&event.into_inner())
            }
        }
    }
}
