use std::convert::Infallible;

use bytes::BytesMut;
use tokio_util::codec::Encoder;
use vector_lib::request_metadata::RequestMetadata;
use vector_lib::{
    config::telemetry,
    event::{EventFinalizers, Finalizable, Metric},
    EstimatedJsonEncodedSizeOf,
};

use super::{encoder::StatsdEncoder, service::StatsdRequest};
use crate::{
    internal_events::SocketMode,
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, IncrementalRequestBuilder,
    },
};

/// Incremental request builder specific to StatsD.
pub struct StatsdRequestBuilder {
    encoder: StatsdEncoder,
    request_max_size: usize,
    encode_buf: BytesMut,
}

impl StatsdRequestBuilder {
    pub fn new(default_namespace: Option<String>, socket_mode: SocketMode) -> Self {
        let encoder = StatsdEncoder::new(default_namespace);
        let request_max_size = match socket_mode {
            // Following the recommended advice [1], we use a datagram size that should reasonably
            // fit within the MTU of the common places that Vector will run: virtual cloud networks,
            // regular ol' Ethernet networks, and so on.
            //
            // [1]: https://github.com/statsd/statsd/blob/0de340f864/docs/metric_types.md?plain=1#L121
            SocketMode::Udp => 1432,

            // Since messages can be much bigger with TCP and Unix domain sockets, we'll give
            // ourselves the chance to build bigger requests which should increase I/O efficiency.
            SocketMode::Tcp | SocketMode::Unix => 8192,
        };

        Self::from_encoder_and_max_size(encoder, request_max_size)
    }

    fn from_encoder_and_max_size(encoder: StatsdEncoder, request_max_size: usize) -> Self {
        Self {
            encoder,
            request_max_size,
            encode_buf: BytesMut::with_capacity(8192),
        }
    }
}

impl Clone for StatsdRequestBuilder {
    fn clone(&self) -> Self {
        Self::from_encoder_and_max_size(self.encoder.clone(), self.request_max_size)
    }
}

impl IncrementalRequestBuilder<Vec<Metric>> for StatsdRequestBuilder {
    type Metadata = (EventFinalizers, RequestMetadata);
    type Payload = Vec<u8>;
    type Request = StatsdRequest;
    type Error = Infallible;

    fn encode_events_incremental(
        &mut self,
        mut input: Vec<Metric>,
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let mut results = Vec::new();
        let mut pending = None;

        let mut metrics = input.drain(..);
        while metrics.len() != 0 || pending.is_some() {
            let mut byte_size = telemetry().create_request_count_byte_size();
            let mut n = 0;

            let mut request_buf = Vec::new();
            let mut finalizers = EventFinalizers::default();
            let mut request_metadata_builder = RequestMetadataBuilder::default();

            loop {
                // Grab the previously pending metric, or the next metric from the drain.
                let (mut metric, was_encoded) = match pending.take() {
                    Some(metric) => (metric, true),
                    None => match metrics.next() {
                        Some(metric) => (metric, false),
                        None => break,
                    },
                };

                byte_size.add_event(&metric, metric.estimated_json_encoded_size_of());

                // Encode the metric. Once we've done that, see if it can fit into the request
                // buffer without exceeding the maximum request size limit.
                //
                // If it doesn't fit, we'll store this metric off to the side and break out of this
                // loop, which will finalize the current request payload and store it in the vector of
                // all generated requests. Otherwise, we'll merge it in and continue encoding.
                //
                // Crucially, we only break out if the current request payload already has data in
                // it, as we need to be able to stick at least one encoded metric into each request.
                if !was_encoded {
                    self.encode_buf.clear();
                    self.encoder
                        .encode(&metric, &mut self.encode_buf)
                        .expect("encoding is infallible");
                }

                let request_buf_len = request_buf.len();
                if request_buf_len != 0
                    && (request_buf_len + self.encode_buf.len() > self.request_max_size)
                {
                    // The metric, as encoded, would cause us to exceed our maximum request size, so
                    // store it off to the side and finalize the current request.
                    pending = Some(metric);
                    break;
                }

                // Merge the encoded metric into the request buffer and take over its event
                // finalizers, etc.
                request_buf.extend(&self.encode_buf[..]);
                finalizers.merge(metric.take_finalizers());
                request_metadata_builder.track_event(metric);
                n += 1;
            }

            // If we encoded one or more metrics this pass, finalize the request.
            if n > 0 {
                let encode_result = EncodeResult::uncompressed(request_buf, byte_size);
                let request_metadata = request_metadata_builder.build(&encode_result);

                results.push(Ok((
                    (finalizers, request_metadata),
                    encode_result.into_payload(),
                )));
            }
        }

        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (finalizers, metadata) = metadata;
        StatsdRequest {
            payload,
            finalizers,
            metadata,
        }
    }
}
