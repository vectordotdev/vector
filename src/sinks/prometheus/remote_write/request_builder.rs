use std::io;

use bytes::{Bytes, BytesMut};
use http::{Request, Uri};
use vector_core::config::telemetry;

use crate::{
    http::Auth,
    sinks::{
        prelude::*,
        prometheus::{collector, PrometheusRemoteWriteAuth},
        util::PartitionInnerBuffer,
    },
};

use super::{sink::RemoteWriteMetric, PartitionKey};

pub(crate) struct RemoteWriteEncoder;

impl encoding::Encoder<Vec<RemoteWriteMetric>> for RemoteWriteEncoder {
    fn encode_input(
        &self,
        input: Vec<RemoteWriteMetric>,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();

        let mut time_series = collector::TimeSeries::new();
        for metric in input {
            byte_size.add_event(&metric.metric, metric.estimated_json_encoded_size_of());

            time_series.encode_metric(
                self.default_namespace.as_deref(),
                &self.buckets,
                &self.quantiles,
                &metric,
            );
        }
        let request = time_series.finish();

        let mut out = BytesMut::with_capacity(request.encoded_len());
        request.encode(&mut out).expect("Out of memory");
        let body = out.freeze();

        write_all(writer, input.len(), body.as_ref())?;

        Ok((body.len(), byte_size))
    }
}

pub(super) struct RemoteWriteRequest {
    pub(super) request: http::Request<Bytes>,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
    tenant_id: String,
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

struct RemoteWriteMetadata {
    finalizers: EventFinalizers,
    tenant_id: Option<String>,
}

pub struct RemoteWriteRequestBuilder {
    pub compression: super::Compression,
    pub encoder: RemoteWriteEncoder,
    pub http_auth: Option<Auth>,
    pub endpoint: String,
}

pub fn build_request(
    method: http::Method,
    endpoint: Uri,
    compression: super::Compression,
    auth: Option<Auth>,
    body: Vec<u8>,
    tenant_id: Option<String>,
) -> Request<hyper::Body> {
    let content_encoding = convert_compression_to_content_encoding(compression);

    let mut builder = http::Request::builder()
        .method(method)
        .uri(endpoint)
        .header("X-Prometheus-Remote-Write-Version", "0.1.0")
        .header("Content-Encoding", content_encoding)
        .header("Content-Type", "application/x-protobuf");

    if let Some(tenant_id) = &tenant_id {
        builder = builder.header("X-Scope-OrgID", tenant_id);
    }

    let mut request = builder.body(body.into()).unwrap();
    if let Some(http_auth) = &auth {
        http_auth.apply(&mut request);
    }

    // if let Some(credentials_provider) = &self.credentials_provider {
    //     sign_request(&mut request, credentials_provider, &self.aws_region).await?;
    // }

    // let (parts, body) = request.into_parts();
    // let request: Request<hyper::Body> = hyper::Request::from_parts(parts, body.into());

    request
}

impl RequestBuilder<PartitionInnerBuffer<Vec<RemoteWriteMetric>, PartitionKey>>
    for RemoteWriteRequestBuilder
{
    type Metadata = RemoteWriteMetadata;
    type Events = Vec<RemoteWriteMetric>;
    type Encoder = RemoteWriteEncoder;
    type Payload = Bytes;
    type Request = RemoteWriteRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: PartitionInnerBuffer<Vec<RemoteWriteMetric>, PartitionKey>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (events, key) = input.into_parts();
        let builder = RequestMetadataBuilder::from_events(&events);
        let metadata = RemoteWriteMetadata {
            finalizers: input.take_finalizers(),
            tenant_id: key,
        };

        (metadata, builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        let request = build_request(
            http::Method::POST,
            &self.endpoint,
            self.compression,
            self.http_auth,
            body,
            metadata.tenant_id,
        );
        RemoteWriteRequest {
            request,
            finalizers: metadata.finalizers,
            tenant_id: metadata.tenant_id,
            metadata,
        }
    }
}

fn compress_block(compression: super::Compression, data: Bytes) -> Vec<u8> {
    match compression {
        super::Compression::Snappy => snap::raw::Encoder::new()
            .compress_vec(&data)
            .expect("snap compression failed, please report"),
        super::Compression::Gzip => {
            let mut buf = Vec::new();
            flate2::read::GzEncoder::new(data.as_ref(), flate2::Compression::default())
                .read_to_end(&mut buf)
                .expect("gzip compression failed, please report");
            buf
        }
        super::Compression::Zstd => {
            zstd::encode_all(data.as_ref(), 0).expect("zstd compression failed, please report")
        }
    }
}

const fn convert_compression_to_content_encoding(compression: super::Compression) -> &'static str {
    match compression {
        super::Compression::Snappy => "snappy",
        super::Compression::Gzip => "gzip",
        super::Compression::Zstd => "zstd",
    }
}
