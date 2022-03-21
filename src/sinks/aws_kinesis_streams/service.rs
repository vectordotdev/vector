use std::task::{Context, Poll};

use futures::{future::BoxFuture, TryFutureExt};
use tower::Service;
use tracing::Instrument;
use vector_core::{internal_event::EventsSent, stream::DriverResponse};

use crate::{
    event::EventStatus,
    internal_events::{AwsKinesisStreamsEventSent, AwsSdkBytesSent},
    sinks::aws_kinesis_streams::request_builder::KinesisRequest,
};
use aws_sdk_kinesis::error::PutRecordsError;
use aws_sdk_kinesis::output::PutRecordsOutput;
use aws_sdk_kinesis::types::SdkError;
use aws_sdk_kinesis::Client as KinesisClient;
use aws_types::region::Region;

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
    type Error = SdkError<PutRecordsError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, requests: Vec<KinesisRequest>) -> Self::Future {
        debug!(
            message = "Sending records.",
            events = %requests.len(),
        );

        let processed_bytes_total = requests
            .iter()
            .map(|req| {
                req.put_records_request
                    .data
                    .as_ref()
                    .map(|data| data.as_ref().len())
                    .unwrap_or(0)
            })
            .sum();
        let events_byte_size = requests.iter().map(|req| req.event_byte_size).sum();
        let count = requests.len();

        let records = requests
            .into_iter()
            .map(|req| req.put_records_request)
            .collect();

        let client = self.client.clone();

        let region = self.region.clone();
        let stream_name = self.stream_name.clone();
        Box::pin(async move {
            let _response: PutRecordsOutput = client
                .put_records()
                .set_records(Some(records))
                .stream_name(stream_name)
                .send()
                .inspect_ok(|_| {
                    emit!(AwsSdkBytesSent {
                        byte_size: processed_bytes_total,
                        region,
                    });

                    // Deprecated
                    emit!(AwsKinesisStreamsEventSent {
                        byte_size: processed_bytes_total
                    });
                })
                .instrument(info_span!("request").or_current())
                .await?;

            Ok(KinesisResponse {
                count,
                events_byte_size,
            })
        })
    }
}
