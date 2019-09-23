use crate::{
    buffers::Acker,
    event::Event,
    region::RegionOrEndpoint,
    sinks::{
        util::retries::{FixedRetryPolicy, RetryLogic},
        Healthcheck, RouterSink,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{Future, Poll, Sink};
use rusoto_core::RusotoFuture;
use rusoto_firehose::{
    DescribeDeliveryStreamError::{self, ResourceNotFound},
    DescribeDeliveryStreamInput, KinesisFirehose, KinesisFirehoseClient, PutRecordBatchError,
    PutRecordBatchInput, PutRecordBatchOutput, Record,
};
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, fmt, sync::Arc, time::Duration};
use tower::Service;
use tracing_futures::{Instrument, Instrumented};

use super::{CoreSinkConfig, Encoding, TowerRequestConfig};

#[derive(Clone)]
pub struct FirehoseService {
    delivery_stream_name: String,
    client: Arc<KinesisFirehoseClient>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct FirehoseConfig {
    pub delivery_stream_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct FirehoseSinkConfig {
    #[serde(default, flatten)]
    pub core_config: CoreSinkConfig,
    #[serde(flatten)]
    pub firehose_config: FirehoseConfig,
    #[serde(default, rename = "request")]
    pub request_config: TowerRequestConfig,
}

#[typetag::serde(name = "aws_kinesis_firehose")]
impl SinkConfig for FirehoseSinkConfig {
    fn build(&self, acker: Acker) -> crate::Result<(RouterSink, Healthcheck)> {
        let config = self.clone();
        let sink = FirehoseService::new(config, acker)?;
        let healthcheck = healthcheck(self.firehose_config.clone())?;
        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

impl FirehoseService {
    pub fn new(
        config: FirehoseSinkConfig,
        acker: Acker,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        let firehose_config = config.firehose_config;
        let request_config = config.request_config;
        let core_config = config.core_config;

        let client = Arc::new(KinesisFirehoseClient::new(
            firehose_config.region.clone().try_into()?,
        ));

        let policy = FixedRetryPolicy::new(
            request_config.retry_attempts,
            Duration::from_secs(request_config.retry_backoff_secs),
            FirehoseRetryLogic,
        );

        let firehose = FirehoseService {
            delivery_stream_name: firehose_config.delivery_stream_name,
            client,
        };

        super::construct(
            core_config,
            request_config,
            firehose,
            acker,
            policy,
            encode_event,
        )
    }
}

fn encode_event(event: Event, encoding: &Encoding) -> Option<Record> {
    let data = super::encode_event(event, encoding);
    Some(Record { data })
}

impl Service<Vec<Record>> for FirehoseService {
    type Response = PutRecordBatchOutput;
    type Error = PutRecordBatchError;
    type Future = Instrumented<RusotoFuture<PutRecordBatchOutput, PutRecordBatchError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, records: Vec<Record>) -> Self::Future {
        debug!(
            message = "sending records.",
            events = %records.len(),
        );

        let request = PutRecordBatchInput {
            records,
            delivery_stream_name: self.delivery_stream_name.clone(),
        };

        self.client
            .put_record_batch(request)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for FirehoseService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FirehoseService")
            .field("delivery_stream_name", &self.delivery_stream_name)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct FirehoseRetryLogic;

impl RetryLogic for FirehoseRetryLogic {
    type Error = PutRecordBatchError;
    type Response = PutRecordBatchOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            PutRecordBatchError::HttpDispatch(_) => true,
            PutRecordBatchError::ServiceUnavailable(_) => true,
            PutRecordBatchError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

type HealthcheckError = super::HealthcheckError<DescribeDeliveryStreamError>;

fn healthcheck(config: FirehoseConfig) -> crate::Result<crate::sinks::Healthcheck> {
    let client = KinesisFirehoseClient::new(config.region.try_into()?);
    let stream_name = config.delivery_stream_name;

    let fut = client
        .describe_delivery_stream(DescribeDeliveryStreamInput {
            delivery_stream_name: stream_name,
            exclusive_start_destination_id: None,
            limit: Some(0),
        })
        .map_err(|source| match source {
            ResourceNotFound(resource) => HealthcheckError::NoMatchingStreamName {
                stream_name: resource,
            }
            .into(),
            other => HealthcheckError::StreamRetrievalFailed { source: other }.into(),
        })
        .and_then(move |res| {
            let description = res.delivery_stream_description;
            let status = &description.delivery_stream_status[..];

            match status {
                "CREATING" | "DELETING" => Err(HealthcheckError::StreamIsNotReady {
                    stream_name: description.delivery_stream_name,
                }
                .into()),
                _ => Ok(()),
            }
        });

    Ok(Box::new(fut))
}
