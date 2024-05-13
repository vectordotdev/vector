use std::num::NonZeroUsize;
use std::sync::Arc;
use std::task::Poll;

use greptimedb_ingester::api::v1::*;
use greptimedb_ingester::Error as GreptimeError;
use vector_lib::event::Metric;

use crate::sinks::greptimedb::{GreptimeDBBatchOutput, GreptimeDBService};
use crate::sinks::prelude::*;

use super::logs::request_builder::log_to_insert_request;
use super::metrics::batch::GreptimeDBBatchSizer;
use super::metrics::request_builder::metric_to_insert_request;

#[derive(Clone)]
pub(super) struct GreptimeDBRequest {
    items: RowInsertRequests,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl GreptimeDBRequest {
    // convert metrics event to GreptimeDBReqesut
    pub(super) fn from_metrics(metrics: Vec<Metric>) -> Self {
        let mut items = Vec::with_capacity(metrics.len());
        let mut finalizers = EventFinalizers::default();
        let mut request_metadata_builder = RequestMetadataBuilder::default();

        let sizer = GreptimeDBBatchSizer;
        let mut estimated_request_size = 0;
        for mut metric in metrics.into_iter() {
            finalizers.merge(metric.take_finalizers());
            estimated_request_size += sizer.estimated_size_of(&metric);

            request_metadata_builder.track_event(metric.clone());

            items.push(metric_to_insert_request(metric));
        }

        let request_size =
            NonZeroUsize::new(estimated_request_size).expect("request should never be zero length");

        GreptimeDBRequest {
            items: RowInsertRequests { inserts: items },
            finalizers,
            metadata: request_metadata_builder.with_request_size(request_size),
        }
    }

    // convert logs event to GreptimeDBRequest
    pub(super) fn from_logs(logs: Vec<LogEvent>, table: String) -> Self {
        let mut items = Vec::with_capacity(logs.len());
        let mut finalizers = EventFinalizers::default();
        let mut request_metadata_builder = RequestMetadataBuilder::default();
        let mut estimated_request_size = 0;
        for mut log in logs.into_iter() {
            finalizers.merge(log.take_finalizers());
            estimated_request_size += log.size_of();

            request_metadata_builder.track_event(log.clone());

            items.push(log_to_insert_request(log, table.clone()));
        }

        let request_size =
            NonZeroUsize::new(estimated_request_size).expect("request should never be zero length");

        GreptimeDBRequest {
            items: RowInsertRequests { inserts: items },
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
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
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
            let metadata = req.metadata;
            let result = client.row_insert(req.items).await?;

            Ok(GreptimeDBBatchOutput {
                _item_count: result,
                metadata,
            })
        })
    }
}
