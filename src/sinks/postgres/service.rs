use std::{
    num::NonZeroUsize,
    task::{Context, Poll},
};

use futures::future::BoxFuture;
use snafu::{ResultExt, Snafu};
use sqlx::{Pool, Postgres, types::Json};
use tower::Service;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::JsonSerializerConfig,
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{
    internal_events::EndpointBytesSent,
    sinks::prelude::{RequestMetadataBuilder, RetryLogic},
};

const POSTGRES_PROTOCOL: &str = "postgres";

#[derive(Clone)]
pub struct PostgresRetryLogic;

impl RetryLogic for PostgresRetryLogic {
    type Error = PostgresServiceError;
    type Request = PostgresRequest;
    type Response = PostgresResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        let PostgresServiceError::Postgres {
            source: postgres_error,
        } = error
        else {
            return false;
        };

        if matches!(
            postgres_error,
            sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut
        ) {
            return true;
        }

        if let sqlx::Error::Database(db_err) = postgres_error {
            if let Some(code) = db_err.code() {
                // Class 08: connection exception
                // Class 57: 57P01 admin_shutdown, 57P02 crash_shutdown, 57P03 cannot_connect_now
                // Class 40: 40001 serialization_failure, 40P01 deadlock_detected
                if code.starts_with("08")
                    || matches!(code.as_ref(), "57P01" | "57P02" | "57P03" | "40001" | "40P01")
                {
                    return true;
                }
            }
        }

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

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, error::Error as StdError, fmt};

    use super::*;

    #[derive(Debug)]
    struct MockDbError {
        code: &'static str,
        message: &'static str,
    }

    impl fmt::Display for MockDbError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.code, self.message)
        }
    }

    impl StdError for MockDbError {}

    impl sqlx::error::DatabaseError for MockDbError {
        fn message(&self) -> &str {
            self.message
        }

        fn as_error(&self) -> &(dyn StdError + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn StdError + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn StdError + Send + Sync + 'static> {
            self
        }

        fn code(&self) -> Option<Cow<'_, str>> {
            Some(Cow::Borrowed(self.code))
        }
    }

    fn make_db_error(code: &'static str) -> PostgresServiceError {
        PostgresServiceError::Postgres {
            source: sqlx::Error::Database(Box::new(MockDbError {
                code,
                message: "mock database error",
            })),
        }
    }

    #[test]
    fn test_postgres_retry_logic() {
        let logic = PostgresRetryLogic;

        // Session termination errors (SQLSTATE 57P01, 57P02, 57P03)
        assert!(logic.is_retriable_error(&make_db_error("57P01")));
        assert!(logic.is_retriable_error(&make_db_error("57P02")));
        assert!(logic.is_retriable_error(&make_db_error("57P03")));

        // Connection exceptions (Class 08)
        assert!(logic.is_retriable_error(&make_db_error("08006")));
        assert!(logic.is_retriable_error(&make_db_error("08001")));
        assert!(logic.is_retriable_error(&make_db_error("08003")));

        // Concurrency conflicts (Class 40)
        assert!(logic.is_retriable_error(&make_db_error("40001")));
        assert!(logic.is_retriable_error(&make_db_error("40P01")));

        // IO and PoolTimedOut errors
        let io_err = PostgresServiceError::Postgres {
            source: sqlx::Error::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "reset",
            )),
        };
        assert!(logic.is_retriable_error(&io_err));

        let pool_err = PostgresServiceError::Postgres {
            source: sqlx::Error::PoolTimedOut,
        };
        assert!(logic.is_retriable_error(&pool_err));

        // Non-retriable database errors
        assert!(!logic.is_retriable_error(&make_db_error("23505"))); // unique_violation
        assert!(!logic.is_retriable_error(&make_db_error("42P01"))); // undefined_table
        assert!(!logic.is_retriable_error(&make_db_error("22001"))); // string_data_right_truncation
    }
}

