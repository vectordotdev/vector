use std::task::{Context, Poll};

use redis::aio::ConnectionManager;

use crate::sinks::prelude::*;

use super::{config::Method, RedisRequest, RedisSinkError};

#[derive(Clone)]
pub struct RedisService {
    pub(super) conn: ConnectionManager,
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

        let mut conn = self.conn.clone();
        let mut pipe = redis::pipe();

        for kv in kvs.request {
            match self.data_type {
                super::DataType::List(method) => match method {
                    Method::LPush => {
                        if count > 1 {
                            pipe.atomic().lpush(kv.key, kv.value.as_ref());
                        } else {
                            pipe.lpush(kv.key, kv.value.as_ref());
                        }
                    }
                    Method::RPush => {
                        if count > 1 {
                            pipe.atomic().rpush(kv.key, kv.value.as_ref());
                        } else {
                            pipe.rpush(kv.key, kv.value.as_ref());
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

        Box::pin(async move {
            match pipe.query_async(&mut conn).await {
                Ok(event_status) => Ok(RedisResponse {
                    event_status,
                    events_byte_size: kvs.metadata.into_events_estimated_json_encoded_byte_size(),
                    byte_size,
                }),
                Err(error) => Err(RedisSinkError::SendError { source: error }),
            }
        })
    }
}

pub struct RedisResponse {
    pub event_status: Vec<bool>,
    pub events_byte_size: GroupedCountByteSize,
    pub byte_size: usize,
}

impl RedisResponse {
    pub(super) fn is_successful(&self) -> bool {
        self.event_status.iter().all(|x| *x)
    }
}

impl DriverResponse for RedisResponse {
    fn event_status(&self) -> EventStatus {
        if self.is_successful() {
            EventStatus::Delivered
        } else {
            EventStatus::Errored
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}
