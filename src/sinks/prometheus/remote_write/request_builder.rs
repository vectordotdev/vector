use std::io;

use bytes::{Bytes, BytesMut};
use prost::Message;
use vector_core::config::telemetry;

use crate::sinks::{
    prelude::*,
    prometheus::{collector, collector::MetricCollector as _},
};

use super::{sink::RemoteWriteMetric, PartitionKey};

pub(crate) struct RemoteWriteEncoder {
    pub(super) default_namespace: Option<String>,
    pub(super) buckets: Vec<f64>,
    pub(super) quantiles: Vec<f64>,
}

impl encoding::Encoder<Vec<RemoteWriteMetric>> for RemoteWriteEncoder {
    fn encode_input(
        &self,
        input: Vec<RemoteWriteMetric>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();

        let mut time_series = collector::TimeSeries::new();
        let len = input.len();
        for metric in input {
            byte_size.add_event(&metric.metric, metric.estimated_json_encoded_size_of());

            time_series.encode_metric(
                self.default_namespace.as_deref(),
                &self.buckets,
                &self.quantiles,
                &metric.metric,
            );
        }
        let request = time_series.finish();

        let mut out = BytesMut::with_capacity(request.encoded_len());
        request.encode(&mut out).expect("Out of memory");
        let body = out.freeze();

        write_all(writer, len, body.as_ref())?;

        Ok((body.len(), byte_size))
    }
}

#[derive(Clone)]
pub(super) struct RemoteWriteRequest {
    pub(super) request: Bytes,
    pub(super) tenant_id: Option<String>,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for RemoteWriteRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for RemoteWriteRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

pub(super) struct RemoteWriteMetadata {
    finalizers: EventFinalizers,
    tenant_id: Option<String>,
}

pub(super) struct RemoteWriteRequestBuilder {
    pub(super) compression: super::Compression,
    pub(super) encoder: RemoteWriteEncoder,
}

impl RequestBuilder<(PartitionKey, Vec<RemoteWriteMetric>)> for RemoteWriteRequestBuilder {
    type Metadata = RemoteWriteMetadata;
    type Events = Vec<RemoteWriteMetric>;
    type Encoder = RemoteWriteEncoder;
    type Payload = Bytes;
    type Request = RemoteWriteRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression.into()
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<RemoteWriteMetric>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;
        let builder = RequestMetadataBuilder::from_events(&events);
        let metadata = RemoteWriteMetadata {
            finalizers: events.take_finalizers(),
            tenant_id: key.tenant_id,
        };

        (metadata, builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        RemoteWriteRequest {
            request: payload.into_payload(),
            finalizers: metadata.finalizers,
            tenant_id: metadata.tenant_id,
            metadata: request_metadata,
        }
    }
}
