use std::task::{Context, Poll};
use futures::future::BoxFuture;
use hyper::service::Service;
use rusoto_core::{Region, RusotoError};
use rusoto_firehose::{KinesisFirehose, KinesisFirehoseClient, PutRecordBatchError, PutRecordBatchInput, Record};
use tracing::Instrument;
use vector_core::internal_event::EventsSent;
use vector_core::stream::DriverResponse;
use crate::buffers::Ackable;
use crate::config::SinkContext;
use crate::event::EventStatus;
use crate::sinks::aws_kinesis_firehose::config::KinesisFirehoseSinkConfig;
use crate::sinks::aws_kinesis_firehose::request_builder::KinesisRequest;

#[derive(Clone)]
pub struct KinesisService {
    pub client: KinesisFirehoseClient,
    pub region: Region,
    pub stream_name: String,
}

// impl KinesisFirehoseService {
//     pub fn new(
//         config: KinesisFirehoseSinkConfig,
//         client: KinesisFirehoseClient,
//         cx: SinkContext,
//     ) -> crate::Result<impl Sink<Event, Error = ()>> {
//         let batch_config = config.batch;
//
//         if batch_config.max_bytes.unwrap_or_default() > MAX_PAYLOAD_SIZE {
//             return Err(Box::new(BuildError::BatchMaxSize));
//         }
//
//         if batch_config.max_events.unwrap_or_default() > MAX_PAYLOAD_EVENTS {
//             return Err(Box::new(BuildError::BatchMaxEvents));
//         }
//
//         let batch = BatchSettings::default()
//             .bytes(4_000_000)
//             .events(500)
//             .timeout(1)
//             .parse_config(batch_config)?;
//
//         let request = config.request.unwrap_with(&TowerRequestConfig::default());
//         let encoding = config.encoding.clone();
//         let kinesis = KinesisFirehoseService { client, config };
//         let sink = request
//             .batch_sink(
//                 KinesisFirehoseRetryLogic,
//                 kinesis,
//                 VecBuffer::new(batch.size),
//                 batch.timeout,
//                 cx.acker(),
//                 sink::StdServiceLogic::default(),
//             )
//             .sink_map_err(|error| error!(message = "Fatal kinesis firehose sink error.", %error))
//             .with_flat_map(move |e| stream::iter(Some(encode_event(e, &encoding))).map(Ok));
//
//         Ok(sink)
//     }
// }


pub struct KinesisResponse {

}

impl DriverResponse for KinesisResponse{
    fn event_status(&self) -> EventStatus {
        todo!()
    }

    fn events_sent(&self) -> EventsSent {
        todo!()
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

        // let processed_bytes_total = requests
        //     .iter()
        //     .map(|req| req.record.data.len())
        //     .sum();
        // let events_byte_size = requests.iter().map(|req| req.event_byte_size).sum();
        let count = requests.len();

        let records = requests
            .into_iter()
            .map(|req| req.record)
            .collect();

        let client = self.client.clone();
        let request = PutRecordBatchInput {
            records,
            delivery_stream_name: self.stream_name.clone(),
        };

        Box::pin(async move {
            client
                .put_record_batch(request)
                .instrument(info_span!("request"))
                .await;

            Ok(KinesisResponse {

            })
        })
    }
}
