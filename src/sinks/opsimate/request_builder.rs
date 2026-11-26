//! `RequestBuilder` implementation for the `opsimate` sink.

use std::io;

use bytes::Bytes;

use super::encoder::OpsiMateEncoder;
use crate::sinks::{prelude::*, util::http::HttpRequest};

pub(super) struct OpsiMateRequestBuilder {
    pub(super) encoder: OpsiMateEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<Vec<Event>> for OpsiMateRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = OpsiMateEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut events: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), metadata, request_metadata, ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_request_carrying_the_encoded_alert() {
        let builder = OpsiMateRequestBuilder {
            encoder: OpsiMateEncoder {
                transformer: Transformer::default(),
            },
            compression: Compression::None,
        };

        let mut log = LogEvent::default();
        log.insert(vrl::event_path!("id"), "alert-1");
        log.insert(vrl::event_path!("alertName"), "Test");
        log.insert(
            vrl::event_path!("tags"),
            Value::Array(vec!["vector".into()]),
        );

        let (finalizers, metadata_builder, events) = builder.split_input(vec![Event::Log(log)]);
        let (encoded, metadata) = builder
            .encode_events(events)
            .map(|encoded| {
                let metadata = metadata_builder.build(&encoded);
                (encoded, metadata)
            })
            .expect("encoding a single event succeeds");

        assert_eq!(metadata.event_count(), 1);

        let mut request = builder.build_request(finalizers, metadata, encoded);
        let payload: serde_json::Value = serde_json::from_slice(&request.take_payload()).unwrap();
        assert_eq!(payload["id"], "alert-1");
        assert_eq!(payload["alertName"], "Test");
        assert!(payload.is_object());
    }
}
