use bytes::Bytes;
use std::io::Write;
use std::task::{Context, Poll};
use tower::Service;
use vector::config::telemetry;
use vector::event::{EstimatedJsonEncodedSizeOf, Event, EventFinalizers, EventStatus, Finalizable};
use vector::sinks::prelude::{
    BoxFuture, Compression, DriverResponse, EncodeResult, GroupedCountByteSize, MetaDescriptive,
    RequestBuilder, RequestMetadata, RequestMetadataBuilder,
};
use vector::sinks::util::encoding::as_tracked_write;
use vector::sinks::util::encoding::Encoder as SinkEncoder;

use vector_lib::Error;

pub struct DummyRequest {
    bytes: Bytes,
    request_metadata: RequestMetadata,
    event_finalizers: EventFinalizers,
}

impl Finalizable for DummyRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.event_finalizers)
    }
}

impl MetaDescriptive for DummyRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

pub struct DummyService {}

impl Service<DummyRequest> for DummyService {
    type Response = DummyServiceResponse;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: DummyRequest) -> Self::Future {
        Box::pin(async move {
            Ok(DummyServiceResponse {
                byte_len: req.bytes.len(),
                request_metadata: req.request_metadata,
            })
        })
    }
}

#[derive(Clone, Debug)]
pub struct DummyLogsEncoder {}

impl SinkEncoder<Vec<Event>> for DummyLogsEncoder {
    fn encode_input(
        &self,
        input: Vec<Event>,
        writer: &mut dyn Write,
    ) -> std::io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        for event in &input {
            byte_size.add_event(event, event.estimated_json_encoded_size_of());
        }
        let written_bytes =
            as_tracked_write::<_, _, std::io::Error>(writer, &input, |writer, value| {
                for item in value {
                    writer.write_all(item.as_log().get_message().unwrap().as_bytes().unwrap())?;
                }
                Ok(())
            })?;
        Ok((written_bytes, byte_size))
    }
}

pub struct DummyRequestBuilder {
    pub(crate) encoder: DummyLogsEncoder,
}

impl RequestBuilder<Vec<Event>> for DummyRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = DummyLogsEncoder;
    type Payload = Bytes;
    type Request = DummyRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = input.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&input);
        (finalizers, builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let bytes = payload.into_payload();
        DummyRequest {
            bytes,
            event_finalizers: metadata,
            request_metadata,
        }
    }
}

pub struct DummyServiceResponse {
    byte_len: usize,
    request_metadata: RequestMetadata,
}

impl DriverResponse for DummyServiceResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.request_metadata
            .events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_len)
    }
}
