use std::{fmt, io, num::NonZeroUsize, sync::Arc};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use codecs::encoding::Framer;
use futures::StreamExt;
use futures_util::stream::BoxStream;
use tokio_util::codec::Encoder as _;
use tower::Service;
use uuid::Uuid;
use vector_common::request_metadata::{GroupedCountByteSize, RequestMetadata};
use vector_core::{
    event::Finalizable,
    partition::Partitioner,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use crate::{
    codecs::{Encoder, Transformer},
    event::Event,
    sinks::{
        s3_common::{
            config::S3Options,
            partitioner::{S3KeyPartitioner, S3PartitionKey},
            service::{S3Metadata, S3Request},
        },
        util::{
            metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression,
            RequestBuilder, SinkBuilderExt,
        },
    },
};

pub struct NewS3Sink<Svc> {
    pub service: Svc,
    pub partitioner: S3KeyPartitioner,
    pub transformer: Transformer,
    pub framer: Framer,
    pub serializer: codecs::encoding::Serializer,
    pub batcher_settings: BatcherSettings,
    pub options: S3RequestOptions,
}

struct EncodedEvent {
    inner: Event,
    encoded: BytesMut,
}

// hack to reuse this trait for encoded size
impl ByteSizeOf for EncodedEvent {
    fn size_of(&self) -> usize {
        self.allocated_bytes()
    }

    fn allocated_bytes(&self) -> usize {
        self.encoded.len()
    }
}

struct WrappedPartitioner(S3KeyPartitioner);

impl Partitioner for WrappedPartitioner {
    type Item = EncodedEvent;
    type Key = Option<S3PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.0.partition(&item.inner)
    }
}

impl<Svc> NewS3Sink<Svc>
where
    Svc: Service<S3Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let transformer = self.transformer;
        let mut serializer = self.serializer;
        let partitioner = WrappedPartitioner(self.partitioner);
        let service = self.service;
        let framer = Arc::new(self.framer);
        let batcher_settings = self.batcher_settings;
        let options = Arc::new(self.options);

        let combined_encoder = Arc::new(Encoder::<Framer>::new(
            framer.as_ref().clone(),
            serializer.clone(),
        ));

        let builder_limit = NonZeroUsize::new(64);

        input
            .map(|event| {
                let mut to_encode = event.clone();
                transformer.transform(&mut to_encode);

                let mut encoded = BytesMut::new();
                serializer.encode(to_encode, &mut encoded).unwrap();

                EncodedEvent {
                    inner: event,
                    encoded,
                }
            })
            .batched_partitioned(partitioner, batcher_settings)
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .concurrent_map(builder_limit, move |(partition_key, encoded_events)| {
                let framer = Arc::clone(&framer);
                let combined_encoder = Arc::clone(&combined_encoder);
                let options = Arc::clone(&options);

                Box::pin(async move {
                    // This is silly because we really just need the prefix, delimiter, and suffix. Oh well.
                    let mut framer = framer.as_ref().clone();

                    let mut grouped_sizes = GroupedCountByteSize::new_tagged();
                    let mut events = Vec::with_capacity(encoded_events.len());
                    let mut encoded = Vec::with_capacity(encoded_events.len());
                    for e in encoded_events {
                        grouped_sizes.add_event(&e.inner, e.encoded.len().into());
                        events.push(e.inner);
                        encoded.push(e.encoded);
                    }

                    // TODO: this doesn't include framing, is that right?
                    let events_encoded_size = encoded.iter().map(BytesMut::len).sum::<usize>();

                    let finalizers = events.take_finalizers();
                    let s3_key_prefix = partition_key.key_prefix.clone();

                    let metadata = S3Metadata {
                        partition_key,
                        s3_key: s3_key_prefix,
                        finalizers,
                    };

                    // TODO: not doing compression yet
                    let mut payload = BytesMut::new();
                    payload.extend_from_slice(combined_encoder.batch_prefix());
                    let mut remaining = encoded.len();
                    for buf in encoded {
                        payload.extend_from_slice(buf.as_ref());
                        remaining -= 1;
                        if remaining > 0 {
                            // write the frame delimiter
                            framer.encode((), &mut payload).expect("framing to bytes");
                        }
                    }
                    payload.extend_from_slice(combined_encoder.batch_suffix());

                    let request_metadata = RequestMetadata::new(
                        events.len(),
                        events_encoded_size,
                        payload.len(),
                        // TODO: same since no compression yet
                        payload.len(),
                        // TODO: just using encoded size here, not sure if we still need to estimate?
                        grouped_sizes,
                    );

                    options.build_request(
                        metadata,
                        request_metadata,
                        EncodeResult::uncompressed(payload.freeze()),
                    )
                })
            })
            .into_driver(service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<Svc> vector_core::sink::StreamSink<Event> for NewS3Sink<Svc>
where
    Svc: Service<S3Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(Clone)]
pub struct S3RequestOptions {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub encoder: (Transformer, Encoder<Framer>),
    pub compression: Compression,
}

impl RequestBuilder<(S3PartitionKey, Vec<Event>)> for S3RequestOptions {
    type Metadata = S3Metadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = S3Request;
    type Error = io::Error; // TODO: this is ugly.

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (S3PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let builder = RequestMetadataBuilder::from_events(&events);

        let finalizers = events.take_finalizers();
        let s3_key_prefix = partition_key.key_prefix.clone();

        let metadata = S3Metadata {
            partition_key,
            s3_key: s3_key_prefix,
            finalizers,
        };

        (metadata, builder, events)
    }

    fn build_request(
        &self,
        mut s3metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let filename = {
            let formatted_ts = Utc::now().format(self.filename_time_format.as_str());

            self.filename_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let ssekms_key_id = s3metadata.partition_key.ssekms_key_id.clone();
        let mut s3_options = self.api_options.clone();
        s3_options.ssekms_key_id = ssekms_key_id;

        let extension = self
            .filename_extension
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.compression.extension().into());

        s3metadata.s3_key = format!("{}{}.{}", s3metadata.s3_key, filename, extension);

        S3Request {
            body: payload.into_payload(),
            bucket: self.bucket.clone(),
            metadata: s3metadata,
            request_metadata,
            content_encoding: self.compression.content_encoding(),
            options: s3_options,
        }
    }
}
