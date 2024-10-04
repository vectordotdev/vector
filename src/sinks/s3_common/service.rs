use std::task::{Context, Poll};

use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::Client as S3Client;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::byte_stream::ByteStream;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::Bytes;
use futures::future::BoxFuture;
use md5::Digest;
use tower::Service;
use tracing::Instrument;
use vector_lib::event::{EventFinalizers, EventStatus, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;

use super::config::S3Options;
use super::partitioner::S3PartitionKey;

#[derive(Debug, Clone)]
pub struct S3Request {
    pub body: Bytes,
    pub bucket: String,
    pub metadata: S3Metadata,
    pub request_metadata: RequestMetadata,
    pub content_encoding: Option<&'static str>,
    pub options: S3Options,
}

impl Finalizable for S3Request {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for S3Request {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone, Debug)]
pub struct S3Metadata {
    pub partition_key: S3PartitionKey,
    pub s3_key: String,
    pub finalizers: EventFinalizers,
}

#[derive(Debug)]
pub struct S3Response {
    events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for S3Response {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

/// Wrapper for the AWS SDK S3 client.
///
/// Provides a `tower::Service`-compatible wrapper around the native
/// AWS SDK S3 Client, allowing it to be composed within a Tower "stack",
/// such that we can easily and transparently provide retries, concurrency
/// limits, rate limits, and more.
#[derive(Clone)]
pub struct S3Service {
    client: S3Client,
}

impl S3Service {
    pub const fn new(client: S3Client) -> S3Service {
        S3Service { client }
    }

    pub fn client(&self) -> S3Client {
        self.client.clone()
    }
}

impl Service<S3Request> for S3Service {
    type Response = S3Response;
    type Error = SdkError<PutObjectError, HttpResponse>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: S3Request) -> Self::Future {
        let options = request.options;

        let content_encoding = request.content_encoding;
        let content_encoding = options
            .content_encoding
            .or_else(|| content_encoding.map(|ce| ce.to_string()));
        let content_type = options
            .content_type
            .or_else(|| Some("text/x-log".to_owned()));

        let content_md5 = BASE64_STANDARD.encode(md5::Md5::digest(&request.body));

        let tagging = options.tags.map(|tags| {
            let mut tagging = url::form_urlencoded::Serializer::new(String::new());
            for (p, v) in &tags {
                tagging.append_pair(p, v);
            }
            tagging.finish()
        });

        let events_byte_size = request
            .request_metadata
            .into_events_estimated_json_encoded_byte_size();

        let client = self.client.clone();

        Box::pin(async move {
            let request = client
                .put_object()
                .body(bytes_to_bytestream(request.body))
                .bucket(request.bucket)
                .key(request.metadata.s3_key)
                .set_content_encoding(content_encoding)
                .set_content_type(content_type)
                .set_acl(options.acl.map(Into::into))
                .set_grant_full_control(options.grant_full_control)
                .set_grant_read(options.grant_read)
                .set_grant_read_acp(options.grant_read_acp)
                .set_grant_write_acp(options.grant_write_acp)
                .set_server_side_encryption(options.server_side_encryption.map(Into::into))
                .set_ssekms_key_id(options.ssekms_key_id)
                .set_storage_class(Some(options.storage_class.into()))
                .set_tagging(tagging)
                .content_md5(content_md5);

            let result = request.send().in_current_span().await;

            result.map(|_| S3Response { events_byte_size })
        })
    }
}

fn bytes_to_bytestream(buf: Bytes) -> ByteStream {
    ByteStream::from(buf)
}
