use crate::sinks::{
    greptimedb::metrics::{
        batch::GreptimeDBBatchSizer,
        request_builder::{metric_to_insert_request, RequestBuilderOptions},
    },
    prelude::*,
};
use greptimedb_ingester::{api::v1::*, Error as GreptimeError};
use std::num::NonZeroUsize;
use vector_lib::event::Metric;

/// GreptimeDBGrpcRequest is a wrapper around the RowInsertRequests
/// that is used to send metrics to GreptimeDB.
/// It also contains the finalizers and metadata that are used to
#[derive(Clone)]
pub(super) struct GreptimeDBGrpcRequest {
    pub(super) items: RowInsertRequests,
    pub(super) finalizers: EventFinalizers,
    pub(super) metadata: RequestMetadata,
}

impl GreptimeDBGrpcRequest {
    // convert metrics event to GreptimeDBGrpcRequest
    pub(super) fn from_metrics(metrics: Vec<Metric>, options: &RequestBuilderOptions) -> Self {
        let mut items = Vec::with_capacity(metrics.len());
        let mut finalizers = EventFinalizers::default();
        let mut request_metadata_builder = RequestMetadataBuilder::default();

        let sizer = GreptimeDBBatchSizer;
        let mut estimated_request_size = 0;
        for mut metric in metrics.into_iter() {
            finalizers.merge(metric.take_finalizers());
            estimated_request_size += sizer.estimated_size_of(&metric);

            request_metadata_builder.track_event(metric.clone());

            items.push(metric_to_insert_request(metric, options));
        }

        let request_size =
            NonZeroUsize::new(estimated_request_size).expect("request should never be zero length");

        GreptimeDBGrpcRequest {
            items: RowInsertRequests { inserts: items },
            finalizers,
            metadata: request_metadata_builder.with_request_size(request_size),
        }
    }
}

impl Finalizable for GreptimeDBGrpcRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for GreptimeDBGrpcRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

/// GreptimeDBGrpcBatchOutput is the output of the [`GreptimeDBGrpcService`]
#[derive(Debug)]
pub struct GreptimeDBGrpcBatchOutput {
    pub _item_count: u32,
    pub metadata: RequestMetadata,
}

impl DriverResponse for GreptimeDBGrpcBatchOutput {
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

/// GreptimeDBGrpcRetryLogic is the retry logic for the [`GreptimeDBGrpcSink`]
#[derive(Clone, Default)]
pub struct GreptimeDBGrpcRetryLogic;

impl RetryLogic for GreptimeDBGrpcRetryLogic {
    type Response = GreptimeDBGrpcBatchOutput;
    type Error = GreptimeError;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }
}
