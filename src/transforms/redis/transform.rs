use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::Stream;
use lru::LruCache;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use vector_lib::{
    lookup::{OwnedTargetPath, lookup_v2::OptionalValuePath},
    stream::ConcurrentMap,
    transform::TaskTransform,
};

use crate::{
    event::Event,
    internal_events::{
        RedisTransformLookupError, RedisTransformLruCacheHit, RedisTransformLruCacheMiss,
        TemplateRenderingError,
    },
    template::Template,
};

/// Redis transform that enriches events with data from Redis lookups.
pub struct RedisTransform {
    connection_manager: std::sync::Arc<redis::aio::ConnectionManager>,
    key_template: Template,
    output_field: OptionalValuePath,
    default_value: Option<String>,
    cache: Option<Arc<Mutex<LruCache<String, (Option<String>, Instant)>>>>,
    cache_ttl: Option<Duration>,
}

impl RedisTransform {
    pub fn new(
        connection_manager: redis::aio::ConnectionManager,
        key_template: Template,
        output_field: OptionalValuePath,
        default_value: Option<String>,
        cache_max_size: Option<NonZeroUsize>,
        cache_ttl: Option<Duration>,
    ) -> Self {
        let cache = cache_max_size.map(|max_size| Arc::new(Mutex::new(LruCache::new(max_size))));

        Self {
            connection_manager: std::sync::Arc::new(connection_manager),
            key_template,
            output_field,
            default_value,
            cache,
            cache_ttl,
        }
    }

    async fn lookup_with_cache(
        cache: Option<Arc<Mutex<LruCache<String, (Option<String>, Instant)>>>>,
        cache_ttl: Option<Duration>,
        connection_manager: std::sync::Arc<redis::aio::ConnectionManager>,
        key: String,
        skip_cache: bool,
    ) -> Result<Option<String>, redis::RedisError> {
        if skip_cache || cache.is_none() {
            return Self::lookup_redis(connection_manager, key).await;
        }

        let cache_ref = cache.unwrap();
        let now = Instant::now();

        {
            let mut cache_guard = cache_ref.lock().await;
            if let Some((cached_value, cached_time)) = cache_guard.get(&key) {
                if let Some(ttl) = cache_ttl {
                    if now.duration_since(*cached_time) < ttl {
                        emit!(RedisTransformLruCacheHit);
                        return Ok(cached_value.clone());
                    } else {
                        cache_guard.pop(&key);
                    }
                } else {
                    emit!(RedisTransformLruCacheHit);
                    return Ok(cached_value.clone());
                }
            }
        }

        emit!(RedisTransformLruCacheMiss);

        let result = Self::lookup_redis(connection_manager, key.clone()).await;

        if let Ok(ref value) = result {
            let mut cache_guard = cache_ref.lock().await;
            cache_guard.put(key, (value.clone(), now));
        }

        result
    }

    async fn lookup_redis(
        connection_manager: std::sync::Arc<redis::aio::ConnectionManager>,
        key: String,
    ) -> Result<Option<String>, redis::RedisError> {
        let mut conn = (*connection_manager).clone();
        conn.get::<_, Option<String>>(&key).await
    }

    fn enrich_event(
        output_field: &OptionalValuePath,
        default_value: &Option<String>,
        mut event: Event,
        value: Option<String>,
    ) -> Event {
        let value_to_insert = value.or_else(|| default_value.clone());

        if let Some(value_str) = value_to_insert {
            if let Some(path) = output_field.path.as_ref() {
                match &mut event {
                    Event::Log(log) => {
                        log.insert(&OwnedTargetPath::event(path.clone()), value_str);
                    }
                    Event::Trace(trace) => {
                        trace.insert(&OwnedTargetPath::event(path.clone()), value_str);
                    }
                    Event::Metric(metric) => {
                        let path_str = path.to_string();
                        let tag_name = path_str
                            .split('.')
                            .last()
                            .unwrap_or("redis_data")
                            .to_string();
                        metric.replace_tag(tag_name, value_str);
                    }
                }
            }
        }
        event
    }
}

impl TaskTransform<Event> for RedisTransform {
    fn transform(
        self: Box<Self>,
        input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let connection_manager = std::sync::Arc::clone(&self.connection_manager);
        let key_template = self.key_template;
        let output_field = self.output_field;
        let default_value = self.default_value;
        let cache = self.cache.clone();
        let cache_ttl = self.cache_ttl;

        let concurrency_limit =
            NonZeroUsize::new(100).expect("concurrency limit must be at least 1");

        Box::pin(ConcurrentMap::new(
            input_rx,
            Some(concurrency_limit),
            move |event| {
                let connection_manager = std::sync::Arc::clone(&connection_manager);
                let key_template = key_template.clone();
                let output_field = output_field.clone();
                let default_value = default_value.clone();
                let cache = cache.clone();
                let cache_ttl = cache_ttl;

                Box::pin(async move {
                    let key = match key_template.render_string(&event) {
                        Ok(k) => k,
                        Err(err) => {
                            emit!(TemplateRenderingError {
                                error: err,
                                field: Some("key"),
                                drop_event: false,
                            });
                            return event;
                        }
                    };

                    let lookup_result =
                        Self::lookup_with_cache(cache, cache_ttl, connection_manager, key, false)
                            .await;

                    match lookup_result {
                        Ok(value) => {
                            Self::enrich_event(&output_field, &default_value, event, value)
                        }
                        Err(err) => {
                            emit!(RedisTransformLookupError {
                                error: format!("Redis lookup failed: {}", err),
                            });
                            event
                        }
                    }
                })
            },
        ))
    }
}
