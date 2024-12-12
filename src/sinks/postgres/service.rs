use std::num::NonZeroUsize;
use std::task::{Context, Poll};

use crate::internal_events::EndpointBytesSent;
use crate::sinks::prelude::{RequestMetadataBuilder, RetryLogic};
use futures::future::BoxFuture;
use sqlx::types::Json;
use sqlx::{Error as PostgresError, Pool, Postgres};
use tower::Service;
use vector_lib::event::{EventFinalizers, EventStatus, Finalizable, LogEvent};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;
use vector_lib::EstimatedJsonEncodedSizeOf;

const POSTGRES_PROTOCOL: &str = "postgres";

#[derive(Clone)]
pub struct PostgresRetryLogic;

impl RetryLogic for PostgresRetryLogic {
    type Error = PostgresError;
    type Response = PostgresResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        // TODO: Implement this
        false
    }
}

#[derive(Clone)]
pub struct PostgresService {
    connection_pool: Pool<Postgres>,
    table: String,
    endpoint: String,
}

impl PostgresService {
    pub const fn new(connection_pool: Pool<Postgres>, table: String, endpoint: String) -> Self {
        Self {
            connection_pool,
            table,
            endpoint,
        }
    }
}

// TODO: do we need this clone?
#[derive(Clone)]
pub struct PostgresRequest {
    pub events: Vec<LogEvent>,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl From<Vec<LogEvent>> for PostgresRequest {
    fn from(mut events: Vec<LogEvent>) -> Self {
        let finalizers = events.take_finalizers();
        let metadata_builder = RequestMetadataBuilder::from_events(&events);
        let events_size = NonZeroUsize::new(events.estimated_json_encoded_size_of().get())
            .expect("payload should never be zero length");
        // TODO: is this metadata creation correct?
        let metadata = metadata_builder.with_request_size(events_size);
        PostgresRequest {
            events,
            finalizers,
            metadata,
        }
    }
}

impl Finalizable for PostgresRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for PostgresRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

pub struct PostgresResponse {
    metadata: RequestMetadata,
}

impl DriverResponse for PostgresResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        // TODO: Is this correct?
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

impl Service<PostgresRequest> for PostgresService {
    type Response = PostgresResponse;
    type Error = PostgresError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PostgresRequest) -> Self::Future {
        let service = self.clone();
        let future = async move {
            let table = service.table;
            let metadata = request.metadata;
            // TODO: If a single item of the batch fails, the whole batch will fail its insert.
            // Is this intended behaviour?
            sqlx::query(&format!(
                "INSERT INTO {table} SELECT * FROM jsonb_populate_recordset(NULL::{table}, $1)"
            ))
            .bind(Json(request.events))
            .execute(&service.connection_pool)
            .await?;

            emit!(EndpointBytesSent {
                byte_size: metadata.request_encoded_size(),
                protocol: POSTGRES_PROTOCOL,
                endpoint: &service.endpoint,
            });

            Ok(PostgresResponse { metadata })
        };

        Box::pin(future)
    }
}
