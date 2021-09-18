use crate::{internal_events::aws_s3::sink::S3EventsSent, serde::to_string};
use bytes::Bytes;
use futures::{future::BoxFuture, stream};
use md5::Digest;
use rusoto_core::{ByteStream, RusotoError};
use rusoto_s3::{PutObjectError, PutObjectOutput, PutObjectRequest, S3Client, S3};
use std::task::{Context, Poll};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::event::{EventFinalizers, EventStatus, Finalizable};

use super::config::S3Options;

#[derive(Debug, Clone)]
pub struct S3Request {
    pub body: Bytes,
    pub bucket: String,
    pub key: String,
    pub content_encoding: Option<&'static str>,
    pub options: S3Options,
    pub batch_size: usize,
    pub finalizers: EventFinalizers,
}

impl Finalizable for S3Request {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug)]
pub struct S3Response {
    inner: PutObjectOutput,
}

impl AsRef<EventStatus> for S3Response {
    fn as_ref(&self) -> &EventStatus {
        &EventStatus::Delivered
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
}

impl S3Service {
    pub const fn new(client: S3Client) -> S3Service {
        S3Service { client }
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

        let body_len = request.body.len();
        let client = self.client.clone();
        let request = PutObjectRequest {
            body: Some(bytes_to_bytestream(request.body)),
            bucket: request.bucket,
            key: request.key,
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

        Box::pin(async move {
            let result = client.put_object(request).in_current_span().await;

            // TODO: This is fine for testing, but we should have a better
            // pattern for this.
            emit!(S3EventsSent {
                byte_size: body_len,
            });

            result.map(|inner| S3Response { inner })
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
