use std::num::NonZeroUsize;
use std::sync::Arc;
use std::task::Poll;

use futures_util::future::BoxFuture;

use greptimedb_client::api::v1::auth_header::AuthScheme;
use greptimedb_client::api::v1::*;
use greptimedb_client::{Client, Database, Error as GreptimeError};
use tower::Service;
use vector_common::finalization::{EventFinalizers, EventStatus, Finalizable};
use vector_common::internal_event::CountByteSize;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_core::event::Metric;
use vector_core::stream::DriverResponse;

use crate::sinks::prelude::RetryLogic;
use crate::sinks::util::metadata::RequestMetadataBuilder;

use super::batch::GreptimeDBBatchSizer;
use super::request_builder::metric_to_insert_request;
use super::GreptimeDBConfig;

#[derive(Clone, Default)]
pub(super) struct GreptimeDBRetryLogic;

impl RetryLogic for GreptimeDBRetryLogic {
    type Response = GreptimeDBBatchOutput;
    type Error = GreptimeError;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }
}

#[derive(Clone)]
pub(super) struct GreptimeDBRequest {
    items: Vec<InsertRequest>,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl GreptimeDBRequest {
    pub(super) fn from_metrics(metrics: Vec<Metric>) -> Self {
        let mut items = Vec::with_capacity(metrics.len());
        let mut finalizers = EventFinalizers::default();
        let mut request_metadata_builder = RequestMetadataBuilder::default();

        let sizer = GreptimeDBBatchSizer::default();
        let mut estimated_request_size = 0;
        for mut metric in metrics.into_iter() {
            finalizers.merge(metric.take_finalizers());
            estimated_request_size += sizer.estimated_size_of(&metric);
            request_metadata_builder.track_event(&metric);
            items.push(metric_to_insert_request(metric));
        }

        let request_size =
            NonZeroUsize::new(estimated_request_size).expect("request should never be zero length");

        GreptimeDBRequest {
            items,
            finalizers,
            metadata: request_metadata_builder.with_request_size(request_size),
        }
    }
}

impl Finalizable for GreptimeDBRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for GreptimeDBRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

#[derive(Debug)]
pub struct GreptimeDBBatchOutput {
    item_count: u32,
    metadata: RequestMetadata,
}

impl DriverResponse for GreptimeDBBatchOutput {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(
            self.item_count as usize,
            self.metadata.events_estimated_json_encoded_byte_size(),
        )
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_wire_size())
    }
}

#[derive(Debug, Clone)]
pub struct GreptimeDBService {
    /// the client that connects to greptimedb
    client: Arc<Database>,
}

impl GreptimeDBService {
    pub fn new(config: &GreptimeDBConfig) -> Self {
        let grpc_client = Client::with_urls(vec![&config.endpoint]);
        let mut client = Database::new_with_dbname(&config.dbname, grpc_client);

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client.set_auth(AuthScheme::Basic(Basic {
                username: username.to_owned(),
                password: password.clone().into(),
            }))
        };

        // TODO: tls configuration

        GreptimeDBService {
            client: Arc::new(client),
        }
    }
}

impl Service<GreptimeDBRequest> for GreptimeDBService {
    type Response = GreptimeDBBatchOutput;
    type Error = GreptimeError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Convert vector metrics into GreptimeDB format and send them in batch
    fn call(&mut self, req: GreptimeDBRequest) -> Self::Future {
        let client = Arc::clone(&self.client);

        Box::pin(async move {
            let metadata = req.get_metadata();
            let result = client.insert(req.items).await?;

            Ok(GreptimeDBBatchOutput {
                item_count: result,
                metadata,
            })
        })
    }
}
