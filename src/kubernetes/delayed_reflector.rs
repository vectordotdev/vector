use futures::{Stream, TryStreamExt};
use kube::api::Resource;
use kube::runtime::reflector::store;
use kube::runtime::watcher;
use std::{hash::Hash, time::Duration};
use tokio_util::time::DelayQueue;

pub fn reflector_shim<K, W>(
    mut store: store::Writer<K>,
    stream: W,
    mut delayer: Delayer<K>,
) -> impl Stream<Item = W::Item>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
    W: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    stream.inspect_ok(move |event| {
        match event {
            watcher::Event::Applied(_) => {
                // Immediately apply event
                store.apply_watcher_event(event);
            }
            watcher::Event::Deleted(_) => {
                // Delay reconciling any `Deleted` events
                delayer.queue.insert(event.to_owned(), delayer.ttl);
            }
            watcher::Event::Restarted(_) => {
                // Clear all delayed events when the cache is refreshed
                delayer.queue.clear();
                store.apply_watcher_event(event);
            }
        } // Check if any events are ready to delete
        while let std::task::Poll::Ready(Some(Ok(expired))) =
            delayer
                .queue
                .poll_expired(&mut futures::task::Context::from_waker(
                    &futures::task::noop_waker(),
                ))
        {
            // Remove expired event from the queue and then pass the event
            // down into the cache to apply
            let event = delayer.queue.remove(&expired.key());
            store.apply_watcher_event(&event.into_inner());
        }
    })
}

pub struct Delayer<K>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
{
    queue: DelayQueue<watcher::Event<K>>,
    ttl: Duration,
}

impl<K> Delayer<K>
where
    K: Resource + Clone,
    K::DynamicType: Eq + Hash + Clone,
{
    pub fn new(ttl: Duration) -> Self {
        Self {
            queue: DelayQueue::default(),
            ttl,
        }
    }
}
