use std::task::{Context, Poll};

use futures::future::BoxFuture;
use hyper::service::Service;
use rusoto_core::{Region, RusotoError};
use rusoto_firehose::{
    KinesisFirehose, KinesisFirehoseClient, PutRecordBatchError, PutRecordBatchInput,
};
use tracing::Instrument;
use vector_core::{internal_event::EventsSent, stream::DriverResponse};

use crate::{
    event::EventStatus, internal_events::AwsBytesSent,
    sinks::aws_kinesis_firehose::request_builder::KinesisRequest,
};

#[derive(Clone)]
pub struct KinesisService {
    pub client: KinesisFirehoseClient,
    pub region: Region,
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

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.count,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

impl Service<Vec<KinesisRequest>> for KinesisService {
    type Response = KinesisResponse;
    type Error = RusotoError<PutRecordBatchError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, requests: Vec<KinesisRequest>) -> Self::Future {
        debug!(
            message = "Sending records.",
            events = %requests.len(),
        );

        let processed_bytes_total = requests.iter().map(|req| req.record.data.len()).sum();
        let events_byte_size = requests.iter().map(|req| req.event_byte_size).sum();
        let count = requests.len();
        let region = self.region.clone();

        let records = requests.into_iter().map(|req| req.record).collect();

        let client = self.client.clone();
        let request = PutRecordBatchInput {
            records,
            delivery_stream_name: self.stream_name.clone(),
        };

        Box::pin(async move {
            client
                .put_record_batch(request)
                .instrument(info_span!("request"))
                .await?;

            emit!(&AwsBytesSent {
                byte_size: processed_bytes_total,
                region
            });

            Ok(KinesisResponse {
                events_byte_size,
                count,
            })
        })
    }
}
