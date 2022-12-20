use super::convert::build_block;
use crate::event::{EventStatus, LogEvent};
use clickhouse_rs::{types::SqlType, Pool};
use futures::future::BoxFuture;
use std::task::{Context, Poll};
use vector_common::{internal_event::CountByteSize, Error};
use vector_core::{stream::DriverResponse, ByteSizeOf};

pub(super) struct ClickhouseResponse {
    pub(super) event_count: usize,
    pub(super) event_byte_size: usize,
}

impl DriverResponse for ClickhouseResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.event_count, self.event_byte_size)
    }
}

pub(super) struct ClickhouseService {
    pool: Pool,
    schema: Vec<(String, SqlType)>,
    table: String,
}

impl tower::Service<Vec<LogEvent>> for ClickhouseService {
    type Response = ClickhouseResponse;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Readiness check of the client is done through the `push_events()`
        // call happening inside `call()`. That check blocks until the client is
        // ready to perform another request.
        //
        // See: <https://docs.rs/tonic/0.4.2/tonic/client/struct.Grpc.html#method.ready>
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, events: Vec<LogEvent>) -> Self::Future {
        let pool = self.pool.clone();
        let event_count = events.len();
        let event_byte_size = events.size_of();
        let schema = self.schema.clone();
        let table = self.table.clone();
        Box::pin(async move {
            let block = build_block(schema, events)?;
            let mut handle = pool.get_handle().await?;
            handle.insert(table, block).await?;
            Ok(ClickhouseResponse {
                event_count,
                event_byte_size,
            })
        })
    }
}

impl ClickhouseService {
    pub(super) fn new(pool: Pool, schema: Vec<(String, SqlType)>, table: String) -> Self {
        Self {
            pool,
            schema,
            table,
        }
    }
}
