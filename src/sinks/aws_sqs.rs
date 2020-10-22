use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    dns::Resolver,
    internal_events::AwsSqsEventSent,
    region::RegionOrEndpoint,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        rusoto,
        sink::Response,
        BatchConfig, BatchSettings, EncodedLength, TowerRequestConfig, VecBuffer,
    },
    Event,
};
use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use futures01::{stream::iter_ok, Sink};
use lazy_static::lazy_static;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use rusoto_core::RusotoError;
use rusoto_sqs::{
    GetQueueAttributesError, GetQueueAttributesRequest, SendMessageBatchError,
    SendMessageBatchRequest, SendMessageBatchRequestEntry, SendMessageBatchResult, Sqs, SqsClient,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    convert::TryInto,
    task::{Context, Poll},
};
use tower::Service;
use tracing_futures::Instrument;

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("GetQueueAttributes failed: {}", source))]
    GetQueueAttributes {
        source: RusotoError<GetQueueAttributesError>,
    },
    #[snafu(display("Queue is not FIFO"))]
    IsNotFifo,
}

#[derive(Clone)]
pub struct SqsSink {
    client: SqsClient,
    queue_url: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SqsSinkConfig {
    pub queue_url: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub assume_role: Option<String>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        timeout_secs: Some(30),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<SqsSinkConfig>("aws_sqs")
}

impl GenerateConfig for SqsSinkConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SinkConfig for SqsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client(cx.resolver())?;
        let healthcheck = self.clone().healthcheck(client.clone());
        let sink = SqsSink::new(self.clone(), cx, client)?;
        Ok((
            super::VectorSink::Futures01Sink(Box::new(sink)),
            healthcheck.boxed(),
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_sqs"
    }
}

impl SqsSinkConfig {
    pub async fn healthcheck(self, client: SqsClient) -> crate::Result<()> {
        client
            .get_queue_attributes(GetQueueAttributesRequest {
                attribute_names: Some(vec!["FifoQueue".to_owned()]),
                queue_url: self.queue_url.clone(),
            })
            .await
            .context(GetQueueAttributes)
            .and_then(|result| {
                if self.queue_url.ends_with(".fifo") {
                    let fifo = result
                        .attributes
                        .and_then(|attrs| attrs.get("FifoQueue").map(|fifo| fifo == "true"))
                        .unwrap_or(false);
                    if !fifo {
                        return Err(HealthcheckError::IsNotFifo);
                    }
                }
                Ok(())
            })
            .map_err(Into::into)
    }

    pub fn create_client(&self, resolver: Resolver) -> crate::Result<SqsClient> {
        let region = (&self.region).try_into()?;
        let client = rusoto::client(resolver)?;

        let creds = rusoto::AwsCredentialsProvider::new(&region, self.assume_role.clone())?;

        Ok(SqsClient::new_with(client, creds, region))
    }
}

impl SqsSink {
    pub fn new(
        config: SqsSinkConfig,
        cx: SinkContext,
        client: SqsClient,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-batch-api-actions.html
        // Up to 10 events, not more than 256KB as total size.
        let batch = BatchSettings::default()
            .events(10)
            .bytes(250_000)
            .timeout(1)
            .parse_config(config.batch)?;

        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();
        let fifo = config.queue_url.ends_with(".fifo");

        let sqs = SqsSink {
            client,
            queue_url: config.queue_url,
        };

        let sink = request
            .batch_sink(
                SqsRetryLogic,
                sqs,
                VecBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
            )
            .sink_map_err(|e| error!("Fatal sqs sink error: {}", e))
            .with_flat_map(move |e| iter_ok(encode_event(e, &encoding, fifo)));

        Ok(sink)
    }
}

impl Service<Vec<SendMessageBatchRequestEntry>> for SqsSink {
    type Response = SendMessageBatchResult;
    type Error = RusotoError<SendMessageBatchError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, entries: Vec<SendMessageBatchRequestEntry>) -> Self::Future {
        debug!(
            message = "sending records.",
            events = %entries.len(),
        );

        let sizes: Vec<usize> = entries
            .iter()
            .map(|entry| entry.message_body.len())
            .collect();

        let client = self.client.clone();
        let request = SendMessageBatchRequest {
            entries,
            queue_url: self.queue_url.clone(),
        };

        Box::pin(async move {
            client
                .send_message_batch(request)
                .inspect_ok(|_| {
                    for byte_size in sizes {
                        emit!(AwsSqsEventSent { byte_size });
                    }
                })
                .instrument(info_span!("request"))
                .await
        })
    }
}

impl EncodedLength for SendMessageBatchRequestEntry {
    fn encoded_length(&self) -> usize {
        self.message_body.len()
    }
}

impl Response for SendMessageBatchResult {}

#[derive(Debug, Clone)]
struct SqsRetryLogic;

impl RetryLogic for SqsRetryLogic {
    type Error = RusotoError<SendMessageBatchError>;
    type Response = SendMessageBatchResult;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::HttpDispatch(_) => true,
            RusotoError::Service(SendMessageBatchError::BatchEntryIdsNotDistinct(_)) => true,
            RusotoError::Service(SendMessageBatchError::TooManyEntriesInBatchRequest(_)) => true,
            RusotoError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

fn encode_event(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
    fifo: bool,
) -> Option<SendMessageBatchRequestEntry> {
    encoding.apply_rules(&mut event);

    let log = event.into_log();
    let message_body = match encoding.codec() {
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| "".into()),
        Encoding::Json => serde_json::to_string(&log).expect("Error encoding event as json."),
    };

    Some(SendMessageBatchRequestEntry {
        id: gen_id(30),
        message_body,
        message_group_id: if fifo { Some(gen_id(128)) } else { None },
        ..Default::default()
    })
}

fn gen_id(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn sqs_encode_event_text() {
        let message = "hello world".to_string();
        let event = encode_event(message.clone().into(), &Encoding::Text.into(), false).unwrap();

        assert_eq!(&event.message_body, &message);
    }

    #[test]
    fn sqs_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let event = encode_event(event, &Encoding::Json.into(), false).unwrap();

        let map: BTreeMap<String, String> = serde_json::from_str(&event.message_body).unwrap();

        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }
}

#[cfg(feature = "aws-sqs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_util::{random_lines_with_stream, random_string};
    use futures::{compat::Sink01CompatExt, SinkExt, StreamExt};
    use rusoto_core::Region;
    use rusoto_sqs::{CreateQueueRequest, GetQueueUrlRequest, ReceiveMessageRequest};
    use std::collections::HashMap;
    use tokio::time::{delay_for, Duration};

    #[tokio::test]
    async fn sqs_send_message_batch() {
        let cx = SinkContext::new_test();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4566".into(),
        };

        let queue_name = gen_queue_name();
        ensure_queue(region.clone(), queue_name.clone()).await;
        let queue_url = get_queue_url(region.clone(), queue_name.clone()).await;

        let client = SqsClient::new(region);

        let config = SqsSinkConfig {
            queue_url: queue_url.clone(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
            encoding: Encoding::Text.into(),
            batch: BatchConfig {
                max_events: Some(2),
                ..Default::default()
            },
            request: Default::default(),
            assume_role: None,
        };

        config.clone().healthcheck(client.clone()).await.unwrap();

        let sink = SqsSink::new(config, cx, client.clone()).unwrap();

        let (mut input_lines, events) = random_lines_with_stream(100, 10);
        let mut events = events.map(Ok);

        sink.sink_compat().send_all(&mut events).await.unwrap();

        delay_for(Duration::from_secs(1)).await;

        let response = client
            .receive_message(ReceiveMessageRequest {
                max_number_of_messages: Some(input_lines.len() as i64),
                queue_url,
                ..Default::default()
            })
            .await
            .unwrap();

        let mut output_lines = response
            .clone()
            .messages
            .unwrap()
            .into_iter()
            .map(|e| e.body.unwrap())
            .collect::<Vec<_>>();

        input_lines.sort();
        output_lines.sort();

        assert_eq!(output_lines, input_lines);
        assert_eq!(input_lines.len(), response.messages.unwrap().len());
    }

    async fn ensure_queue(region: Region, queue_name: String) {
        let client = SqsClient::new(region);

        let attributes: Option<HashMap<String, String>> = if queue_name.ends_with(".fifo") {
            let mut hash_map = HashMap::new();
            hash_map.insert("FifoQueue".into(), "true".into());
            Some(hash_map)
        } else {
            None
        };

        let req = CreateQueueRequest {
            attributes,
            queue_name,
            tags: None,
        };

        if let Err(error) = client.create_queue(req).await {
            println!("Unable to check the queue {:?}", error);
        }
    }

    async fn get_queue_url(region: Region, queue_name: String) -> String {
        let client = SqsClient::new(region);

        let req = GetQueueUrlRequest {
            queue_name,
            queue_owner_aws_account_id: None,
        };

        client.get_queue_url(req).await.unwrap().queue_url.unwrap()
    }

    fn gen_queue_name() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
