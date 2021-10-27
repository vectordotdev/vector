use std::task::{Context, Poll};
use futures::future::BoxFuture;
use futures::TryFutureExt;
use rusoto_core::RusotoError;
use rusoto_kinesis::{Kinesis, KinesisClient, PutRecordsError, PutRecordsInput, PutRecordsOutput};
use tower::Service;
use tracing::Instrument;
use crate::sinks::aws_kinesis_streams::config::KinesisSinkConfig;
use crate::sinks::aws_kinesis_streams::encode_event;
use crate::sinks::aws_kinesis_streams::request_builder::KinesisRequest;
use crate::sinks::util::sink;
use std::fmt;
use vector_core::internal_event::EventsSent;
use vector_core::stream::DriverResponse;
use crate::event::EventStatus;
use crate::internal_events::AwsKinesisStreamsEventSent;


#[derive(Clone)]
pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}

// impl KinesisService {
//     pub fn new(
//         config: KinesisSinkConfig,
//         client: KinesisClient,
//         cx: SinkContext,
//     ) -> crate::Result<impl Sink<Event, Error = ()>> {
//         let batch = BatchSettings::default()
//             .bytes(5_000_000)
//             .events(500)
//             .timeout(1)
//             .parse_config(config.batch)?;
//         let request = config.request.unwrap_with(&TowerRequestConfig::default());
//         let encoding = config.encoding.clone();
//         let partition_key_field = config.partition_key_field.clone();
//
//         let kinesis = KinesisService { client, config };
//
//         let sink = request
//             .batch_sink(
//                 KinesisRetryLogic,
//                 kinesis,
//                 VecBuffer::new(batch.size),
//                 batch.timeout,
//                 cx.acker(),
//                 sink::StdServiceLogic::default(),
//             )
//             .sink_map_err(|error| error!(message = "Fatal kinesis streams sink error.", %error))
//             .with_flat_map(move |e| {
//                 stream::iter(encode_event(e, &partition_key_field, &encoding)).map(Ok)
//             });
//
//         Ok(sink)
//     }
// }

pub struct KinesisResponse {
    count:
}

impl DriverResponse for KinesisResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        todo!()
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

        let processed_bytes_total = requests.iter().map(|req|req.put_records_request.data.len()).sum();

        let records = requests.iter().map(|req|req.put_records_request).collect();

        let client = self.client.clone();
        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        Box::pin(async move {
            let response:() = client
                .put_records(request)
                .inspect_ok(|_| {
                    emit!(&AwsKinesisStreamsEventSent { processed_bytes_total });
                })
                .instrument(info_span!("request"))
                .await?;
            Ok(KinesisResponse {

            })
        })
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
    }
}
