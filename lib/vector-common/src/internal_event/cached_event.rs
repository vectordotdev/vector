use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use super::{InternalEventHandle, RegisterInternalEvent};

/// Metrics (eg. `component_sent_event_bytes_total`) may need to emit tags based on
/// values contained within the events. These tags can't be determined in advance.
///
/// Metrics need to be registered and the handle needs to be held onto in order to
/// prevent them from expiring and being dropped (this would result in the counter
/// resetting to zero).
/// `CachedEvent` is used to maintain a store of these registered metrics. When a
/// new event is emitted for a previously unseen set of tags an event is registered
/// and stored in the cache.
pub struct CachedEvent<Tags, Event: RegisterInternalEvent> {
    cache: Arc<RwLock<BTreeMap<Tags, <Event as RegisterInternalEvent>::Handle>>>,
}

/// This trait must be implemented by events that emit dynamic tags. `register` must
/// be implemented to register an event based on the set of tags passed.
pub trait RegisterEvent<Tags>: RegisterInternalEvent {
    fn register(tags: &Tags) -> <Self as RegisterInternalEvent>::Handle;
}

/// Deriving `Clone` for `Cached` doesn't work since the `Event` type is not clone,
/// we can happily implement our own `clone` however since we are just cloning
/// the `Arc`.
/// Worth noting that this is a cheap clone since the cache itself is stored behind
/// an `Arc`.
impl<Tags, Event: RegisterInternalEvent> Clone for CachedEvent<Tags, Event> {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

impl<Tags, Event, EventHandle, Data> Default for CachedEvent<Tags, Event>
where
    Data: Sized,
    EventHandle: InternalEventHandle<Data = Data>,
    Tags: Ord + Clone,
    Event: RegisterInternalEvent<Handle = EventHandle> + RegisterEvent<Tags>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Tags, Event, EventHandle, Data> CachedEvent<Tags, Event>
where
    Data: Sized,
    EventHandle: InternalEventHandle<Data = Data>,
    Tags: Ord + Clone,
    Event: RegisterInternalEvent<Handle = EventHandle> + RegisterEvent<Tags>,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Arc::default(),
        }
    }

    /// Emits the event with the given tags.
    /// It will register the event and store in the cache if this has not already
    /// been done.
    ///
    /// # Panics
    ///
    /// This will panic if the lock is poisoned.
    pub fn emit(&self, tags: &Tags, value: Data) {
        let read = self.cache.read().unwrap();
        if let Some(event) = read.get(tags) {
            event.emit(value);
        } else {
            let event = <Event as RegisterEvent<Tags>>::register(tags);
            event.emit(value);

            // Ensure the read lock is dropped so we can write.
            drop(read);
            self.cache.write().unwrap().insert(tags.clone(), event);
        }
    }
}
