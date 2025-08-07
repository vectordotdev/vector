use std::{future::ready, num::NonZeroUsize, pin::Pin, time::Instant};

use futures::{Stream, StreamExt};
use lru::LruCache;

use crate::{event::Event, internal_events::DedupeEventsDropped, transforms::TaskTransform};

use super::{
    common::{FieldMatchConfig, TimedCacheConfig},
    transform::{build_cache_entry, CacheEntry},
};

#[derive(Clone)]
pub struct TimedDedupe {
    fields: FieldMatchConfig,
    cache: LruCache<CacheEntry, Instant>,
    time_config: TimedCacheConfig,
}

impl TimedDedupe {
    pub fn new(
        num_entries: NonZeroUsize,
        fields: FieldMatchConfig,
        time_config: TimedCacheConfig,
    ) -> Self {
        Self {
            fields,
            cache: LruCache::new(num_entries),
            time_config,
        }
    }

    pub fn transform_one(&mut self, event: Event) -> Option<Event> {
        let cache_entry = build_cache_entry(&event, &self.fields);
        let now = Instant::now();
        let drop_event = match self.cache.get(&cache_entry) {
            Some(&time) => {
                let drop = now.duration_since(time) < self.time_config.max_age_ms;
                if self.time_config.refresh_on_drop || !drop {
                    self.cache.put(cache_entry, now);
                }
                drop
            }
            None => {
                self.cache.put(cache_entry, now);
                false
            }
        };
        if drop_event {
            emit!(DedupeEventsDropped { count: 1 });
            None
        } else {
            Some(event)
        }
    }
}

impl TaskTransform<Event> for TimedDedupe {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = self;
        Box::pin(task.filter_map(move |v| ready(inner.transform_one(v))))
    }
}
