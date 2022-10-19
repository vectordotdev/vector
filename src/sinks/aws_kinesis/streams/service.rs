use std::task::{Context, Poll};

use aws_sdk_kinesis::output::PutRecordsOutput;
use aws_sdk_kinesis::types::SdkError;
use aws_sdk_kinesis::Client as KinesisClient;
use aws_sdk_kinesis::{error::PutRecordsError, model::PutRecordsRequestEntry};
use aws_types::region::Region;
use futures::future::BoxFuture;
use tower::Service;
use tracing::Instrument;
use vector_common::request_metadata::MetaDescriptive;
use vector_core::{internal_event::CountByteSize, stream::DriverResponse};

use super::{request_builder::Record, sink::BatchKinesisRequest};
use crate::event::EventStatus;

#[derive(Clone)]
pub struct KinesisService {
    pub client: KinesisClient,
    pub stream_name: String,
    pub region: Option<Region>,
}

pub struct KinesisResponse {
    count: usize,
    events_byte_size: usize,
}

impl DriverResponse for KinesisResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.count, self.events_byte_size)
    }
}

impl<R> Service<BatchKinesisRequest<R>> for KinesisService
where
    R: Record<T = PutRecordsRequestEntry> + std::clone::Clone,
{
    type Response = KinesisResponse;
    type Error = SdkError<PutRecordsError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, requests: BatchKinesisRequest<R>) -> Self::Future {
        let events_byte_size = requests.get_metadata().events_byte_size();
        let count = requests.get_metadata().event_count();

        let records = requests
            .events
            .into_iter()
            .map(|req| req.record.get())
            .collect();

        let client = self.client.clone();

        let stream_name = self.stream_name.clone();
        Box::pin(async move {
            let _response: PutRecordsOutput = client
                .put_records()
                .set_records(Some(records))
                .stream_name(stream_name)
                .send()
                .instrument(info_span!("request").or_current())
                .await?;

            Ok(KinesisResponse {
                count,
                events_byte_size,
            })
        })
    }
}
