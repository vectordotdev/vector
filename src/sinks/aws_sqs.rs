use crate::{
    dns::Resolver,
    event::{self, Event},
    internal_events::AwsSqsEventSent,
    region::RegionOrEndpoint,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        rusoto2::{self, AwsCredentialsProvider},
        sink::Response,
        BatchEventsConfig, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use futures01::{stream::iter_ok, Future, Poll, Sink};
use lazy_static::lazy_static;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use rusoto_core::{Region, RusotoError, RusotoFuture};
use rusoto_sqs::{
    GetQueueAttributesRequest, SendMessageBatchError, SendMessageBatchRequest,
    SendMessageBatchRequestEntry, SendMessageBatchResult, Sqs, SqsClient,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{convert::TryInto, fmt};
use tower::Service;
use tracing_futures::{Instrument, Instrumented};

#[derive(Clone)]
pub struct SqsService {
    client: SqsClient,
    config: SqsSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SqsSinkConfig {
    pub queue_url: String,
    pub fifo: Option<bool>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub batch: BatchEventsConfig,
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

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new_without_default::<SqsSinkConfig>("aws_sqs")
}

#[typetag::serde(name = "aws_sqs")]
impl SinkConfig for SqsSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let config = self.clone();
        let healthcheck = healthcheck(self.clone(), cx.resolver())?;
        let sink = SqsService::new(config, cx)?;
        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_sqs"
    }
}

impl SqsService {
    pub fn new(
        config: SqsSinkConfig,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        let client = create_client(
            config.region.clone().try_into()?,
            config.assume_role.clone(),
            cx.resolver(),
        )?;

        let batch = config.batch.unwrap_or(10, 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();

        let fifo = config.fifo.unwrap_or(false);

        let sqs = SqsService { client, config };

        let sink = request
            .batch_sink(SqsRetryLogic, sqs, Vec::new(), batch, cx.acker())
            .sink_map_err(|e| error!("Fatal sqs sink error: {}", e))
            .with_flat_map(move |e| iter_ok(encode_event(e, &encoding, fifo)));

        Ok(sink)
    }
}

impl Service<Vec<SendMessageBatchRequestEntry>> for SqsService {
    type Response = SendMessageBatchResult;
    type Error = RusotoError<SendMessageBatchError>;
    type Future = Instrumented<RusotoFuture<SendMessageBatchResult, SendMessageBatchError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, entries: Vec<SendMessageBatchRequestEntry>) -> Self::Future {
        debug!(
            message = "sending records.",
            events = %entries.len(),
        );

        let request = SendMessageBatchRequest {
            entries,
            queue_url: self.config.queue_url.clone(),
        };

        self.client
            .send_message_batch(request)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for SqsService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqsService")
            .field("config", &self.config)
            .finish()
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

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("SendMessageBatchError failed: {}", source))]
    SendBatchError {
        source: RusotoError<rusoto_sqs::SendMessageBatchError>,
    },

    #[snafu(display("GetQueueAttributes failed: {}", source))]
    QueueAttributesError {
        source: RusotoError<rusoto_sqs::GetQueueAttributesError>,
    },

    #[snafu(display("CreateQueueError failed: {}", source))]
    CreateQueueError {
        source: RusotoError<rusoto_sqs::CreateQueueError>,
    },
}

fn healthcheck(config: SqsSinkConfig, resolver: Resolver) -> crate::Result<super::Healthcheck> {
    let client = create_client(
        config.region.try_into()?,
        config.assume_role.clone(),
        resolver,
    )?;
    let queue_url = config.queue_url;

    let fut = client
        .get_queue_attributes(GetQueueAttributesRequest {
            attribute_names: None,
            queue_url: queue_url.clone(),
        })
        .map_err(|source| HealthcheckError::QueueAttributesError { source }.into())
        .and_then(move |res| {
            println!("{:#?}", res);
            Ok(())
        });

    Ok(Box::new(fut))
}

fn create_client(
    region: Region,
    assume_role: Option<String>,
    resolver: Resolver,
) -> crate::Result<SqsClient> {
    let client = rusoto2::client(resolver)?;
    let creds = AwsCredentialsProvider::new(&region, assume_role)?;

    Ok(SqsClient::new_with(client, creds, region))
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
            .get(&event::log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| "".into()),
        Encoding::Json => serde_json::to_string(&log).expect("Error encoding event as json."),
    };

    emit!(AwsSqsEventSent {
        byte_size: Bytes::from(message_body.clone()).len()
    });

    let message_group_id = if fifo { Some(gen_id()) } else { None };

    let message_deduplication_id = if fifo { Some(gen_id()) } else { None };

    Some(SendMessageBatchRequestEntry {
        id: gen_id(),
        message_group_id,
        message_deduplication_id,
        message_body,
        ..Default::default()
    })
}

fn gen_id() -> String {
    let rand_id: String = thread_rng().sample_iter(&Alphanumeric).take(30).collect();
    rand_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
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

        assert_eq!(map[&event::log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }
}

#[cfg(feature = "aws-sqs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        region::RegionOrEndpoint,
        test_util::{random_lines_with_stream, random_string, runtime},
        topology::config::SinkContext,
    };
    use rusoto_core::Region;
    use rusoto_sqs::{ReceiveMessageRequest, Sqs, SqsClient};
    use std::collections::HashMap;

    #[test]
    fn sqs_send_message_batch() {
        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4576".into(),
        };

        let queue_name = gen_queue_name();
        ensure_queue(region.clone(), queue_name.clone(), false);
        let queue_url = get_queue_url(region.clone(), queue_name.clone());

        let client = SqsClient::new(region);

        let config = SqsSinkConfig {
            queue_url: queue_url.clone(),
            fifo: Some(false),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4576".into()),
            encoding: Encoding::Text.into(),
            batch: BatchEventsConfig {
                max_events: Some(2),
                timeout_secs: None,
            },
            request: Default::default(),
            assume_role: None,
        };

        let sink = SqsService::new(config, cx).unwrap();

        let (mut input_lines, events) = random_lines_with_stream(100, 10);

        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let response = rt
            .block_on(client.receive_message(ReceiveMessageRequest {
                max_number_of_messages: Some(input_lines.len() as i64),
                queue_url,
                visibility_timeout: None,
                wait_time_seconds: None,
                attribute_names: None,
                receive_request_attempt_id: None,
                message_attribute_names: None,
            }))
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

    fn ensure_queue(region: Region, queue_name: String, fifo: bool) {
        let client = SqsClient::new(region);

        let attributes: Option<HashMap<String, String>> = if fifo {
            let mut hash_map = HashMap::new();
            hash_map.insert("FifoQueue".into(), "true".into());

            Some(hash_map)
        } else {
            None
        };

        let req = rusoto_sqs::CreateQueueRequest {
            attributes,
            queue_name,
            tags: None,
        };

        match client.create_queue(req).sync() {
            Ok(_) => (),
            Err(e) => println!("Unable to check the queue {:?}", e),
        };
    }

    fn get_queue_url(region: Region, queue_name: String) -> String {
        let client = SqsClient::new(region);

        let req = rusoto_sqs::GetQueueUrlRequest {
            queue_name,
            queue_owner_aws_account_id: None,
        };

        client.get_queue_url(req).sync().unwrap().queue_url.unwrap()
    }

    fn gen_queue_name() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
