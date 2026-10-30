use std::{
    fmt,
    task::{Context, Poll},
};

use super::{
    RedisRequest, RedisSinkError,
    config::{ListMethod, SortedSetMethod},
    sink::{ConnectionState, RedisConnection},
};
use crate::sinks::prelude::*;

#[derive(Clone)]
pub struct RedisService {
    pub(super) conn: RedisConnection,
    pub(super) data_type: super::DataType,
}

impl Service<RedisRequest> for RedisService {
    type Response = RedisResponse;
    type Error = RedisSinkError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, kvs: RedisRequest) -> Self::Future {
        let count = kvs.request.len();

        let mut redis_conn = self.conn.clone();
        let mut pipe = redis::pipe();

        for kv in kvs.request {
            match self.data_type {
                super::DataType::List(method) => match method {
                    ListMethod::LPush => {
                        if count > 1 {
                            pipe.atomic().lpush(kv.key, kv.value.as_ref());
                        } else {
                            pipe.lpush(kv.key, kv.value.as_ref());
                        }
                    }
                    ListMethod::RPush => {
                        if count > 1 {
                            pipe.atomic().rpush(kv.key, kv.value.as_ref());
                        } else {
                            pipe.rpush(kv.key, kv.value.as_ref());
                        }
                    }
                },
                super::DataType::SortedSet(method) => match method {
                    SortedSetMethod::ZAdd => {
                        // https://github.com/vectordotdev/vector/issues/23659
                        #[allow(
                            clippy::cast_precision_loss,
                            reason = "Preserve Redis score rounding."
                        )]
                        let score = kv.score.unwrap_or(0) as f64;
                        if count > 1 {
                            pipe.atomic().zadd(kv.key, kv.value.as_ref(), score);
                        } else {
                            pipe.zadd(kv.key, kv.value.as_ref(), score);
                        }
                    }
                },
                super::DataType::Channel => {
                    if count > 1 {
                        pipe.atomic().publish(kv.key, kv.value.as_ref());
                    } else {
                        pipe.publish(kv.key, kv.value.as_ref());
                    }
                }
            }
        }

        let byte_size = kvs.metadata.events_byte_size();
        let is_sorted_set = matches!(self.data_type, super::DataType::SortedSet(_));

        Box::pin(async move {
            let ConnectionState {
                connection: mut conn,
                generation,
            } = redis_conn.get_connection_manager().await?;

            match pipe.query_async::<Vec<bool>>(&mut conn).await {
                Ok(mut event_status) => {
                    // ZADD returns zero when it successfully updates an existing member.
                    if is_sorted_set {
                        event_status.fill(true);
                    }
                    Ok(RedisResponse {
                        event_status: kvs.delivery.record(&event_status),
                        events_byte_size: kvs
                            .metadata
                            .into_events_estimated_json_encoded_byte_size(),
                        byte_size,
                        error: None,
                    })
                }
                Err(error) => Err(RedisSinkError::SendError {
                    source: error,
                    generation,
                }),
            }
        })
    }
}

/// Preserves delivery progress when the retry service returns a terminal error.
pub(super) struct RedisResponseService<S> {
    inner: S,
}

impl<S> RedisResponseService<S> {
    pub(super) const fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Service<RedisRequest> for RedisResponseService<S>
where
    S: Service<RedisRequest, Response = RedisResponse, Error = crate::Error>,
    S::Future: Send + 'static,
{
    type Response = RedisResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: RedisRequest) -> Self::Future {
        let delivery = request.delivery.clone();
        let byte_size = request.metadata.events_byte_size();
        let events_byte_size = request
            .metadata
            .events_estimated_json_encoded_byte_size()
            .clone();
        let future = self.inner.call(request);

        Box::pin(async move {
            Ok(future.await.unwrap_or_else(|error| RedisResponse {
                event_status: delivery.snapshot(),
                events_byte_size,
                byte_size,
                error: Some(error),
            }))
        })
    }
}

pub struct RedisResponse {
    pub event_status: Vec<bool>,
    pub events_byte_size: GroupedCountByteSize,
    pub byte_size: usize,
    pub error: Option<crate::Error>,
}

impl RedisResponse {
    pub(super) fn is_successful(&self) -> bool {
        self.error.is_none() && self.event_status.iter().all(|x| *x)
    }
}

impl DriverResponse for RedisResponse {
    fn event_status(&self) -> EventStatus {
        if self.error.is_some() {
            EventStatus::Rejected
        } else if self.is_successful() {
            EventStatus::Delivered
        } else {
            EventStatus::Errored
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn events_dropped(&self) -> Option<usize> {
        Some(self.event_status.iter().filter(|status| !**status).count())
    }

    fn error(&self) -> Option<&dyn fmt::Debug> {
        self.error.as_ref().map(|error| error as &dyn fmt::Debug)
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}
