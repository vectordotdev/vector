use std::task::{Context, Poll};

use futures::{future::BoxFuture, TryFutureExt};
use rusoto_core::{Region, RusotoError};
use rusoto_kinesis::{Kinesis, KinesisClient, PutRecordsError, PutRecordsInput, PutRecordsOutput};
use tower::Service;
use tracing::Instrument;
use vector_core::{internal_event::EventsSent, stream::DriverResponse};

use crate::{
    event::EventStatus,
    internal_events::{AwsBytesSent, AwsKinesisStreamsEventSent},
    sinks::aws_kinesis_streams::request_builder::KinesisRequest,
};

#[derive(Clone)]
pub struct KinesisService {
    pub client: KinesisClient,
    pub stream_name: String,
    pub region: Region,
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
    type Error = RusotoError<PutRecordsError>;
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
            .map(|req| req.put_records_request.data.len())
            .sum();
        let events_byte_size = requests.iter().map(|req| req.event_byte_size).sum();
        let count = requests.len();

        let records = requests
            .into_iter()
            .map(|req| req.put_records_request)
            .collect();

        let client = self.client.clone();
        let request = PutRecordsInput {
            records,
            stream_name: self.stream_name.clone(),
        };

        let region = self.region.clone();
        Box::pin(async move {
            let _response: PutRecordsOutput = client
                .put_records(request)
                .inspect_ok(|_| {
                    emit!(&AwsBytesSent {
                        byte_size: processed_bytes_total,
                        region,
                    });

                    // Deprecated
                    emit!(&AwsKinesisStreamsEventSent {
                        byte_size: processed_bytes_total
                    });
                })
                .instrument(info_span!("request"))
                .await?;

            Ok(KinesisResponse {
                count,
                events_byte_size,
            })
        })
    }
}
