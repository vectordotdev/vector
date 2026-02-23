use std::{
    num::NonZeroUsize,
    task::{Context, Poll},
};

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use snafu::{ResultExt, Snafu};
use tower::Service;
use uuid::Uuid;
use ydb::{TableClient, Value, YdbError, ydb_struct};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::JsonSerializerConfig,
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{
    internal_events::EndpointBytesSent,
    sinks::prelude::RequestMetadataBuilder,
};

const YDB_PROTOCOL: &str = "ydb";

#[derive(Clone)]
pub struct YdbRetryLogic;

impl crate::sinks::util::retries::RetryLogic for YdbRetryLogic {
    type Error = YdbServiceError;
    type Request = YdbRequest;
    type Response = YdbResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            YdbServiceError::Ydb { source } => {
                matches!(
                    source,
                    YdbError::TransportDial(_) | YdbError::Transport(_) | YdbError::TransportGRPCStatus(_)
                )
            }
            YdbServiceError::VectorCommon { .. } | YdbServiceError::Serialization { .. } => false,
        }
    }
}

#[derive(Clone)]
pub struct YdbService {
    table_client: TableClient,
    table_path: String,
    endpoint: String,
}

impl YdbService {
    pub const fn new(table_client: TableClient, table_path: String, endpoint: String) -> Self {
        Self {
            table_client,
            table_path,
            endpoint,
        }
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
    #[snafu(display("YDB error: {source}"))]
    Ydb { source: YdbError },

    #[snafu(display("Event conversion error: {source}"))]
    VectorCommon { source: vector_common::Error },

    #[snafu(display("Event serialization error: {source}"))]
    Serialization { source: serde_json::Error },
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
        let future = async move {
            let table_path = service.table_path;
            let metadata = request.metadata;
            
            let rows: Result<Vec<Value>, YdbServiceError> = request
                .events
                .into_iter()
                .map(|event| event_to_ydb_value(event))
                .collect();
            
            let rows = rows?;

            service
                .table_client
                .retry_execute_bulk_upsert(table_path, rows)
                .await
                .context(YdbSnafu)?;

            emit!(EndpointBytesSent {
                byte_size: metadata.request_encoded_size(),
                protocol: YDB_PROTOCOL,
                endpoint: &service.endpoint,
            });

            Ok(YdbResponse { metadata })
        };

        Box::pin(future)
    }
}

/// Convert Vector Event to YDB Value
fn event_to_ydb_value(event: Event) -> Result<Value, YdbServiceError> {
    let id = Uuid::now_v7().to_string();
    let id_hash = xxhash_rust::xxh32::xxh32(id.as_bytes(), 0);
    
    let timestamp: DateTime<Utc> = match &event {
        Event::Log(log) => log
            .get_timestamp()
            .and_then(|v| v.as_timestamp())
            .map(|ts| *ts),
        Event::Metric(metric) => metric.timestamp(),
        Event::Trace(_) => None,
    }
    .unwrap_or_else(Utc::now);
    
    let (host, message) = match &event {
        Event::Log(log) => {
            let host = log
                .get("host")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_else(|| String::from("unknown"));
            let message = log
                .get_message()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_else(String::new);
            (host, message)
        }
        _ => (String::new(), String::new()),
    };
    
    let json_serializer = JsonSerializerConfig::default().build();
    let payload_value = json_serializer
        .to_json_value(event)
        .context(VectorCommonSnafu)?;
    
    let payload_json = serde_json::to_string(&payload_value)
        .context(SerializationSnafu)?;
    
    Ok(ydb_struct!(
        "id" => Value::Text(id),
        "id_hash" => Value::Uint32(id_hash),
        "timestamp" => Value::Timestamp(timestamp.into()),
        "host" => Value::Text(host),
        "message" => Value::Text(message),
        "payload" => Value::JsonDocument(payload_json),
    ))
}
