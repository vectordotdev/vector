use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use derivative::Derivative;

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
#[derive(Derivative)]
#[derivative(Clone(bound = ""), Default(bound = "T: Default"))]
pub struct RegisteredEventCache<T: Clone, Event: RegisterTaggedInternalEvent> {
    fixed_tags: T,
    cache: Arc<
        RwLock<
            BTreeMap<
                <Event as RegisterTaggedInternalEvent>::Tags,
                <Event as RegisterInternalEvent>::Handle,
            >,
        >,
    >,
}

/// This trait must be implemented by events that emit dynamic tags. `register` must
/// be implemented to register an event based on the set of tags passed.
pub trait RegisterTaggedInternalEvent: RegisterInternalEvent {
    /// The type that will contain the data necessary to extract the tags
    /// that will be used when registering the event.
    type Tags;

    /// The type that contains data necessary to extract the tags that will
    /// be fixed and only need setting up front when the cache is first created.
    type Fixed;

    fn register(fixed: Self::Fixed, tags: Self::Tags) -> <Self as RegisterInternalEvent>::Handle;
}

impl<Event, EventHandle, Data, Tags, FixedTags> RegisteredEventCache<FixedTags, Event>
where
    Data: Sized,
    EventHandle: InternalEventHandle<Data = Data>,
    Tags: Ord + Clone,
    FixedTags: Clone,
    Event: RegisterInternalEvent<Handle = EventHandle>
        + RegisterTaggedInternalEvent<Tags = Tags, Fixed = FixedTags>,
{
    /// Create a new event cache with a set of fixed tags. These tags are passed to
    /// all registered events.
    pub fn new(fixed_tags: FixedTags) -> Self {
        Self {
            fixed_tags,
            cache: Default::default(),
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
            let event = <Event as RegisterTaggedInternalEvent>::register(
                self.fixed_tags.clone(),
                tags.clone(),
            );
            event.emit(value);

            // Ensure the read lock is dropped so we can write.
            drop(read);
            self.cache.write().unwrap().insert(tags.clone(), event);
        }
    }
}
