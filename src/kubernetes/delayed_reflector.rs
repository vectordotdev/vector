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
        dbg!(">>>>>> entered shim");
        match event {
            watcher::Event::Applied(_) => {
                // Immediately apply event
                dbg!(">>>>>> Event::Applied received");
                store.apply_watcher_event(event);
            }
            watcher::Event::Deleted(_) => {
                // Delay reconciling any `Deleted` events
                dbg!(">>>>>> Event::Deleted received");
                delayer.queue.insert(event.to_owned(), delayer.ttl);
            }
            watcher::Event::Restarted(_) => {
                // Clear all delayed events when the cache is refreshed
                dbg!(">>>>>> Event::Restarted received");
                delayer.queue.clear();
                store.apply_watcher_event(event);
            }
        } // Check if any events are ready to delete
        dbg!(">>>>>> polling delayer");
        while let std::task::Poll::Ready(Some(Ok(expired))) =
            delayer
                .queue
                .poll_expired(&mut futures::task::Context::from_waker(
                    &futures::task::noop_waker(),
                ))
        {
            let event = delayer.queue.remove(expired);
            store.apply_watcher_event(&event.into_inner());
        }
        dbg!(">>>>>> exited shim");
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
