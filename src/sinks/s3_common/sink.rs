use std::{fmt, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use codecs::encoding::{Framer, Serializer};
use futures::stream::BoxStream;
use futures_util::StreamExt;
use tokio_util::codec::Encoder as _;
use tower::Service;
use vector_common::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_core::{
    config::telemetry,
    event::Finalizable,
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::Transformer,
    event::Event,
    sinks::util::{encoding::write_all, Compression, SinkBuilderExt},
};
use crate::{internal_events::SinkRequestBuildError, sinks::util::Compressor};

pub struct S3Sink<Svc, RB, Part> {
    service: Svc,
    request_builder: RB,
    partitioner: Part,
    transformer: Transformer,
    serializer: Serializer,
    framer: Framer,
    batcher_settings: BatcherSettings,
}

impl<Svc, RB, Part> S3Sink<Svc, RB, Part> {
    pub const fn new(
        service: Svc,
        request_builder: RB,
        partitioner: Part,
        transformer: Transformer,
        serializer: Serializer,
        framer: Framer,
        batcher_settings: BatcherSettings,
    ) -> Self {
        Self {
            partitioner,
            service,
            request_builder,
            transformer,
            serializer,
            framer,
            batcher_settings,
        }
    }
}

pub struct EncodedEvent {
    pub original: Event,
    pub json_size: GroupedCountByteSize,
    pub encoded: BytesMut,
}

impl Finalizable for EncodedEvent {
    fn take_finalizers(&mut self) -> vector_common::finalization::EventFinalizers {
        // TODO: Do we need to worry about finalizer getting cloned prior to encoding?
        self.original.take_finalizers()
    }
}

pub trait RequestBuilder {
    type Request;
    type Metadata;
    type PartitionKey;

    /// Compression to be used when building the payload
    fn compression(&self) -> Compression;

    /// Calculate any necessary request metadata from the original events
    fn build_metadata(
        &self,
        partition_key: Self::PartitionKey,
        events: Vec<Event>,
    ) -> Self::Metadata;

    /// Build a full request given metadata and payload.
    ///
    /// `RequestMetadata` here is mostly just a pass-thru field that we should try to factor out
    /// into a common layer.
    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: Bytes,
        request_metadata: RequestMetadata,
    ) -> Self::Request;
}

/// Handles the logic for turning an event into an encoded byte string. This encapsulates both
/// `Transformer` (to handle `{only,except}_fields`) and `Serializer` from the codecs crate, as well
/// as the calculation of estimated JSON size that needs to happen between those two phases.
///
/// We use an `Encoder` around the serializer here because that's where the API is exposed right
/// now. Ideally this struct lets us isolate the complexity of that interface to make it easier to
/// change in the future.
struct EventEncoder {
    transformer: Transformer,
    encoder: crate::codecs::Encoder<()>,
}

impl EventEncoder {
    const fn new(transformer: Transformer, serializer: Serializer) -> Self {
        let encoder = crate::codecs::Encoder::<()>::new(serializer);
        Self {
            transformer,
            encoder,
        }
    }

    fn encode(&mut self, mut event: Event) -> EncodedEvent {
        let original = event.clone();
        self.transformer.transform(&mut event);

        // TODO: It's potentially not ideal to be creating a new one of these per event and then
        // merging them all later, but we do need access to the event after being transformed. Some
        // other options would be to pass along that version of the event, or come up with some
        // cheaper, single-event representation of the data here. Or it might be fine as-is.
        let mut json_size = telemetry().create_request_count_byte_size();
        json_size.add_event(&event, event.estimated_json_encoded_size_of());

        let mut encoded = BytesMut::new();
        self.encoder
            .serialize(event, &mut encoded)
            .expect("writing to memory");

        EncodedEvent {
            original,
            json_size,
            encoded,
        }
    }
}

impl<Svc, RB, Part, Req> S3Sink<Svc, RB, Part>
where
    Svc: Service<Req> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<Request = Req> + Send + Sync + 'static,
    RB::PartitionKey: Send + 'static,
    Part: Partitioner<Item = EncodedEvent, Key = Option<RB::PartitionKey>> + Send + Unpin + 'static,
    Part::Key: Eq + std::hash::Hash + Clone + Send + 'static,
    Req: MetaDescriptive + Finalizable + Send + 'static,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = self.partitioner;
        let settings = self.batcher_settings;
        let mut event_encoder = EventEncoder::new(self.transformer, self.serializer.clone());

        let payload_builder = Arc::new(PayloadBuilder::new(
            self.serializer,
            self.framer,
            self.request_builder.compression(),
        ));
        let builder_limit = NonZeroUsize::new(64);
        let request_builder = Arc::new(self.request_builder);

        input
            .map(|event| event_encoder.encode(event))
            .batched_partitioned_with_size_fn(partitioner, settings, |e| e.encoded.len())
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .concurrent_map(builder_limit, move |(partition_key, events)| {
                let payload_builder = Arc::clone(&payload_builder);
                let request_builder = Arc::clone(&request_builder);

                Box::pin(async move {
                    let event_count = events.len();

                    let mut original_events = Vec::with_capacity(event_count);
                    let mut encoded_events = Vec::with_capacity(event_count);
                    let mut events_estimated_json_size =
                        telemetry().create_request_count_byte_size();
                    for e in events {
                        original_events.push(e.original);
                        encoded_events.push(e.encoded);
                        events_estimated_json_size = events_estimated_json_size + &e.json_size;
                    }
                    let events_byte_size = original_events.size_of();

                    let metadata = request_builder.build_metadata(partition_key, original_events);

                    // Encode the events.
                    let payload = payload_builder.build_payload(encoded_events)?;

                    // Note: it would be nice for the RequestMetadataBuilder to build be created from the
                    // events here, and not need to be required by split_input(). But this then requires
                    // each Event type to implement Serialize, and that causes conflicts with the Serialize
                    // implementation for EstimatedJsonEncodedSizeOf.

                    // Build the request metadata.
                    let request_metadata = RequestMetadata::new(
                        event_count,
                        events_byte_size,
                        payload.uncompressed_size,
                        payload.body.len(),
                        events_estimated_json_size,
                    );

                    // Now build the actual request.
                    // TODO: how does this fail for other sinks? can it not?
                    Ok::<_, crate::Error>(request_builder.build_request(
                        metadata,
                        payload.body,
                        request_metadata,
                    ))
                })
            })
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

struct PayloadBuilder {
    framer: Framer,
    compression: Compression,
    encoder: crate::codecs::Encoder<Framer>,
}

struct Payload {
    body: Bytes,
    uncompressed_size: usize,
}

impl PayloadBuilder {
    fn new(serializer: Serializer, framer: Framer, compression: Compression) -> Self {
        // We don't actually use the serializer at all, we just need an `Encoder` for the batch
        // prefix and suffix, which for some reason you can't get from the framer alone.
        let encoder = crate::codecs::Encoder::<Framer>::new(framer.clone(), serializer);
        Self {
            framer,
            compression,
            encoder,
        }
    }

    fn build_payload(&self, encoded_events: Vec<BytesMut>) -> crate::Result<Payload> {
        let mut compressor = Compressor::from(self.compression);

        let mut remaining = encoded_events.len();
        let mut bytes_written = 0;

        let batch_prefix = self.encoder.batch_prefix();
        write_all(&mut compressor, remaining, batch_prefix)?;
        bytes_written += batch_prefix.len();
        for mut event in encoded_events {
            // TODO: Not framing the final value matches existing behavior but is almost certainly a
            // bug for some types of framing (e.g. length-delimited) and questionable for others
            // (e.g. newline- and character-delimited). Really it only seems appropriate for
            // something like JSON arrays where trailing commas are disallowed.
            if remaining > 0 {
                // This is confusing, but how our codecs work currently. We pass the serialized
                // payload as the destination BytesMut, which the framer then splits and rewrites
                // with framing (really just the framing delimiter; we have to handle batch prefix
                // and suffix ourselves) into the original.
                self.framer.clone().encode((), &mut event)?
            }
            write_all(&mut compressor, remaining, &event)?;
            bytes_written += event.len();
            remaining -= 1;
        }
        let batch_suffix = self.encoder.batch_suffix();
        write_all(&mut compressor, remaining, batch_suffix)?;
        bytes_written += batch_suffix.len();

        Ok(Payload {
            body: compressor.into_inner().freeze(),
            uncompressed_size: bytes_written,
        })
    }
}

#[async_trait]
impl<Svc, RB, Part, Req> StreamSink<Event> for S3Sink<Svc, RB, Part>
where
    Svc: Service<Req> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
    RB: RequestBuilder<Request = Req> + Send + Sync + 'static,
    RB::PartitionKey: Send + 'static,
    Part: Partitioner<Item = EncodedEvent, Key = Option<RB::PartitionKey>> + Send + Unpin + 'static,
    Part::Key: Eq + std::hash::Hash + Clone + Send + 'static,
    Req: MetaDescriptive + Finalizable + Send + 'static,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
