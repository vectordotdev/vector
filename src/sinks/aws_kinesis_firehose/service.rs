use std::task::{Context, Poll};

use aws_sdk_firehose::{
    error::PutRecordBatchError, types::SdkError, Client as KinesisFirehoseClient, Region,
};
use futures::future::BoxFuture;
use hyper::service::Service;
use tracing::Instrument;
use vector_core::{internal_event::CountByteSize, stream::DriverResponse};

use super::request_builder::KinesisRequest;
use crate::event::EventStatus;

#[derive(Clone)]
pub struct KinesisService {
    pub client: KinesisFirehoseClient,
    pub region: Option<Region>,
    pub stream_name: String,
}

pub struct KinesisResponse {
    events_byte_size: usize,
    count: usize,
}

impl DriverResponse for KinesisResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.count, self.events_byte_size)
    }
}

impl Service<Vec<KinesisRequest>> for KinesisService {
    type Response = KinesisResponse;
    type Error = SdkError<PutRecordBatchError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, requests: Vec<KinesisRequest>) -> Self::Future {
        let events_byte_size = requests.iter().map(|req| req.event_byte_size).sum();
        let count = requests.len();

        let records = requests.into_iter().map(|req| req.record).collect();

        let client = self.client.clone();

        let stream_name = self.stream_name.clone();
        Box::pin(async move {
            client
                .put_record_batch()
                .set_records(Some(records))
                .delivery_stream_name(stream_name)
                .send()
                .instrument(info_span!("request").or_current())
                .await?;

            Ok(KinesisResponse {
                events_byte_size,
                count,
            })
        })
    }
}
