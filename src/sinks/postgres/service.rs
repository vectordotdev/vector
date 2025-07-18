use std::num::NonZeroUsize;
use std::task::{Context, Poll};

use crate::internal_events::EndpointBytesSent;
use crate::sinks::prelude::{RequestMetadataBuilder, RetryLogic};
use futures::future::BoxFuture;
use snafu::{ResultExt, Snafu};
use sqlx::types::Json;
use sqlx::{Pool, Postgres};
use tower::Service;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::event::{Event, EventFinalizers, EventStatus, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;
use vector_lib::EstimatedJsonEncodedSizeOf;

const POSTGRES_PROTOCOL: &str = "postgres";

#[derive(Clone)]
pub struct PostgresRetryLogic;

impl RetryLogic for PostgresRetryLogic {
    type Error = PostgresServiceError;
    type Response = PostgresResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        let PostgresServiceError::Postgres {
            source: postgres_error,
        } = error
        else {
            return false;
        };

        matches!(
            postgres_error,
            sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut
        )
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

#[derive(Clone)]
pub struct PostgresRequest {
    pub events: Vec<Event>,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl TryFrom<Vec<Event>> for PostgresRequest {
    type Error = String;

    fn try_from(mut events: Vec<Event>) -> Result<Self, Self::Error> {
        let finalizers = events.take_finalizers();
        let metadata_builder = RequestMetadataBuilder::from_events(&events);
        let events_size = NonZeroUsize::new(events.estimated_json_encoded_size_of().get())
            .ok_or("payload should never be zero length")?;
        let metadata = metadata_builder.with_request_size(events_size);
        Ok(PostgresRequest {
            events,
            finalizers,
            metadata,
        })
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
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

#[derive(Debug, Snafu)]
pub enum PostgresServiceError {
    #[snafu(display("Database error: {source}"))]
    Postgres { source: sqlx::Error },

    #[snafu(display("Serialization error: {source}"))]
    VectorCommon { source: vector_common::Error },
}

impl Service<PostgresRequest> for PostgresService {
    type Response = PostgresResponse;
    type Error = PostgresServiceError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PostgresRequest) -> Self::Future {
        let service = self.clone();
        let future = async move {
            let table = service.table;
            let metadata = request.metadata;
            let json_serializer = JsonSerializerConfig::default().build();
            let serialized_values = request
                .events
                .into_iter()
                .map(|event| json_serializer.to_json_value(event))
                .collect::<Result<Vec<_>, _>>()
                .context(VectorCommonSnafu)?;

            sqlx::query(&format!(
                "INSERT INTO {table} SELECT * FROM jsonb_populate_recordset(NULL::{table}, $1)"
            ))
            .bind(Json(serialized_values))
            .execute(&service.connection_pool)
            .await
            .context(PostgresSnafu)?;

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
