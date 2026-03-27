use std::{
    num::NonZeroUsize,
    sync::{Arc, RwLock},
    task::{Context, Poll},
};

use futures::future::BoxFuture;
use snafu::{ResultExt, Snafu};
use tower::Service;
use tracing::warn;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};
use ydb::{TableClient, TableDescription, YdbError};

use crate::{internal_events::EndpointBytesSent, sinks::prelude::RequestMetadataBuilder};

use super::request::{YdbRequestError, YdbRequestHandler};

const YDB_PROTOCOL: &str = "ydb";

#[derive(Clone)]
pub struct YdbRetryLogic;

impl crate::sinks::util::retries::RetryLogic for YdbRetryLogic {
    type Error = YdbServiceError;
    type Request = YdbRequest;
    type Response = YdbResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            YdbServiceError::Request { source } => match source {
                YdbRequestError::Ydb { source } => {
                    matches!(
                        source,
                        YdbError::TransportDial(_)
                            | YdbError::Transport(_)
                            | YdbError::TransportGRPCStatus(_)
                    )
                }
                YdbRequestError::Mapping { .. } => false,
            },
        }
    }
}

#[derive(Clone)]
pub struct YdbService {
    table_client: TableClient,
    table_path: String,
    endpoint: String,
    table_schema: Arc<RwLock<TableDescription>>,
}

impl YdbService {
    pub fn new(
        table_client: TableClient,
        table_path: String,
        endpoint: String,
        table_schema: TableDescription,
    ) -> Self {
        Self {
            table_client,
            table_path,
            endpoint,
            table_schema: Arc::new(RwLock::new(table_schema)),
        }
    }

    async fn refresh_schema(&self) -> Result<TableDescription, YdbError> {
        self.table_client
            .describe_table(self.table_path.clone())
            .await
    }
}

#[derive(Clone)]
pub struct YdbRequest {
    pub events: Vec<Event>,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl TryFrom<Vec<Event>> for YdbRequest {
    type Error = String;

    fn try_from(mut events: Vec<Event>) -> Result<Self, Self::Error> {
        let finalizers = events.take_finalizers();
        let metadata_builder = RequestMetadataBuilder::from_events(&events);
        let events_size = NonZeroUsize::new(events.estimated_json_encoded_size_of().get())
            .ok_or("payload should never be zero length")?;
        let metadata = metadata_builder.with_request_size(events_size);
        Ok(YdbRequest {
            events,
            finalizers,
            metadata,
        })
    }
}

impl Finalizable for YdbRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for YdbRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

pub struct YdbResponse {
    metadata: RequestMetadata,
}

impl DriverResponse for YdbResponse {
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
pub enum YdbServiceError {
    #[snafu(display("Request error: {source}"))]
    Request { source: YdbRequestError },
}

impl Service<YdbRequest> for YdbService {
    type Response = YdbResponse;
    type Error = YdbServiceError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: YdbRequest) -> Self::Future {
        let service = self.clone();
        let future = async move { service.execute_with_retry(request).await };

        Box::pin(future)
    }
}

impl YdbService {
    async fn execute_with_retry(
        &self,
        request: YdbRequest,
    ) -> Result<YdbResponse, YdbServiceError> {
        let metadata = request.metadata.clone();
        let events = request.events.clone();

        let schema = self
            .table_schema
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Schema lock was poisoned, recovering");
                poisoned.into_inner()
            })
            .clone();

        let result = self.try_execute(events.clone(), &schema).await;

        if let Err(ref e) = result {
            if should_refresh_schema(e) {
                warn!(
                    message = "Operation failed, refreshing schema and retrying",
                    error = %e,
                    table = %self.table_path,
                );

                match self.refresh_schema().await {
                    Ok(new_schema) => {
                        *self.table_schema.write().unwrap_or_else(|poisoned| {
                            warn!("Schema lock was poisoned during refresh, recovering");
                            poisoned.into_inner()
                        }) = new_schema.clone();

                        return self
                            .try_execute(events, &new_schema)
                            .await
                            .map(|_| YdbResponse { metadata });
                    }
                    Err(refresh_err) => {
                        warn!(
                            message = "Schema refresh failed",
                            refresh_error = %refresh_err,
                        );
                        return result.map(|_| YdbResponse { metadata });
                    }
                }
            } else {
                return result.map(|_| YdbResponse { metadata });
            }
        }

        result?;

        emit!(EndpointBytesSent {
            byte_size: metadata.request_encoded_size(),
            protocol: YDB_PROTOCOL,
            endpoint: &self.endpoint,
        });

        Ok(YdbResponse { metadata })
    }

    async fn try_execute(
        &self,
        events: Vec<Event>,
        schema: &TableDescription,
    ) -> Result<(), YdbServiceError> {
        let handler = YdbRequestHandler::prepare(events, schema, self.table_path.clone())
            .context(RequestSnafu)?;

        handler
            .execute(&self.table_client)
            .await
            .context(RequestSnafu)
    }
}

const fn should_refresh_schema(err: &YdbServiceError) -> bool {
    match err {
        YdbServiceError::Request { source } => match source {
            YdbRequestError::Ydb { source } => !matches!(
                source,
                YdbError::TransportDial(_)
                    | YdbError::Transport(_)
                    | YdbError::TransportGRPCStatus(_)
            ),
            YdbRequestError::Mapping { .. } => false,
        },
    }
}
