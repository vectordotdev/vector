use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{future::BoxFuture, stream};
use md5::Digest;
use rusoto_core::{ByteStream, Region, RusotoError};
use rusoto_s3::{PutObjectError, PutObjectRequest, S3Client, S3};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
    stream::DriverResponse,
};

use super::config::S3Options;
use crate::{internal_events::AwsBytesSent, serde::to_string};

#[derive(Debug, Clone)]
pub struct S3Request {
    pub body: Bytes,
    pub bucket: String,
    pub metadata: S3Metadata,
    pub content_encoding: Option<&'static str>,
    pub options: S3Options,
}

impl Ackable for S3Request {
    fn ack_size(&self) -> usize {
        self.metadata.count
    }
}

impl Finalizable for S3Request {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

#[derive(Clone, Debug)]
pub struct S3Metadata {
    pub partition_key: String,
    pub count: usize,
    pub byte_size: usize,
    pub finalizers: EventFinalizers,
}

#[derive(Debug)]
pub struct S3Response {
    count: usize,
    events_byte_size: usize,
}

impl DriverResponse for S3Response {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.count,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

/// Wrapper for the Rusoto S3 client.
///
/// Provides a `tower::Service`-compatible wrapper around the native
/// `rusoto_s3::S3Client`, allowing it to be composed within a Tower "stack",
/// such that we can easily and transparently provide retries, concurrency
/// limits, rate limits, and more.
#[derive(Clone)]
pub struct S3Service {
    client: S3Client,
    region: Region,
}

impl S3Service {
    pub const fn new(client: S3Client, region: Region) -> S3Service {
        S3Service { client, region }
    }

    pub fn client(&self) -> S3Client {
        self.client.clone()
    }
}

impl Service<S3Request> for S3Service {
    type Response = S3Response;
    type Error = RusotoError<PutObjectError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: S3Request) -> Self::Future {
        let options = request.options;

        let content_encoding = request.content_encoding;
        let content_encoding = options
            .content_encoding
            .or_else(|| content_encoding.map(|ce| ce.to_string()));
        let content_type = options
            .content_type
            .or_else(|| Some("text/x-log".to_owned()));

        let content_md5 = base64::encode(md5::Md5::digest(&request.body));

        let mut tagging = url::form_urlencoded::Serializer::new(String::new());
        if let Some(tags) = options.tags {
            for (p, v) in tags {
                tagging.append_pair(&p, &v);
            }
        }
        let tagging = tagging.finish();
        let count = request.metadata.count;
        let events_byte_size = request.metadata.byte_size;

        let request_size = request.body.len();
        let client = self.client.clone();
        let request = PutObjectRequest {
            body: Some(bytes_to_bytestream(request.body)),
            bucket: request.bucket,
            key: request.metadata.partition_key,
            content_encoding,
            content_type,
            acl: options.acl.map(to_string),
            grant_full_control: options.grant_full_control,
            grant_read: options.grant_read,
            grant_read_acp: options.grant_read_acp,
            grant_write_acp: options.grant_write_acp,
            server_side_encryption: options.server_side_encryption.map(to_string),
            ssekms_key_id: options.ssekms_key_id,
            storage_class: options.storage_class.map(to_string),
            tagging: Some(tagging),
            content_md5: Some(content_md5),
            ..Default::default()
        };

        let region = self.region.clone();
        Box::pin(async move {
            client
                .put_object(request)
                .in_current_span()
                .await
                .map(|_inner| {
                    emit!(&AwsBytesSent {
                        byte_size: request_size,
                        region,
                    });
                    S3Response {
                        count,
                        events_byte_size,
                    }
                })
        })
    }
}

fn bytes_to_bytestream(buf: Bytes) -> ByteStream {
    // We _have_ to provide the size hint, because without it, Rusoto can't
    // generate the Content-Length header which is required for the S3 PutObject
    // API call.
    let len = buf.len();
    ByteStream::new_with_size(Box::pin(stream::once(async move { Ok(buf) })), len)
}
