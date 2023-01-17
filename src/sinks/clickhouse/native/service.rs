use super::convert::build_block;
use crate::event::EventStatus;
use clickhouse_rs::{types::SqlType, Pool};
use futures::future::BoxFuture;
use std::task::{Context, Poll};
use tower::Service;
use vector_common::finalization::{EventFinalizers, Finalizable};
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_common::{internal_event::CountByteSize, Error};
use vector_core::event::Event;
use vector_core::stream::DriverResponse;

pub(super) struct NativeClickhouseResponse {
    pub(super) event_count: usize,
    pub(super) event_byte_size: usize,
}

impl DriverResponse for NativeClickhouseResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.event_count, self.event_byte_size)
    }
}

#[derive(Clone, Default)]
pub(super) struct NativeClickhouseRequest {
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
    pub events: Vec<Event>,
}

impl Finalizable for NativeClickhouseRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for NativeClickhouseRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

pub(super) struct ClickhouseService {
    pool: Pool,
    schema: Vec<(String, SqlType)>,
    table: String,
}

impl Service<NativeClickhouseRequest> for ClickhouseService {
    type Response = NativeClickhouseResponse;
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

    fn call(&mut self, request: NativeClickhouseRequest) -> Self::Future {
        let pool = self.pool.clone();
        let event_count = request.metadata.event_count();
        let event_byte_size = request.metadata.events_byte_size();
        let schema = self.schema.clone();
        let table = self.table.clone();
        Box::pin(async move {
            let block = build_block(schema, request.events)?;
            let mut handle = pool.get_handle().await?;
            handle.insert(table, block).await?;
            Ok(NativeClickhouseResponse {
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
