use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    internal_events::{AwsSqsEventSent, AwsSqsMessageGroupIdMissingKeys},
    rusoto::{self, AWSAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::Response,
        BatchSettings, EncodedLength, TowerRequestConfig, VecBuffer,
    },
    template::{Template, TemplateError},
    Event,
};
use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt};
use lazy_static::lazy_static;
use rusoto_core::RusotoError;
use rusoto_sqs::{
    GetQueueAttributesError, GetQueueAttributesRequest, SendMessageError, SendMessageRequest,
    SendMessageResult, Sqs, SqsClient,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    convert::{TryFrom, TryInto},
    task::{Context, Poll},
};
use tower::Service;
use tracing_futures::Instrument;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("`message_group_id` should be defined for FIFO queue."))]
    MessageGroupIdMissing,
    #[snafu(display("`message_group_id` is not allowed with non-FIFO queue."))]
    MessageGroupIdNotAllowed,
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateError },
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("GetQueueAttributes failed: {}", source))]
    GetQueueAttributes {
        source: RusotoError<GetQueueAttributesError>,
    },
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
    pub message_group_id: Option<String>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AWSAuthentication,
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

impl GenerateConfig for SqsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"queue_url = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
            region = "us-east-2"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SinkConfig for SqsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client()?;
        let healthcheck = self.clone().healthcheck(client.clone());
        let sink = SqsSink::new(self.clone(), cx, client)?;
        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck.boxed()))
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
                attribute_names: None,
                queue_url: self.queue_url.clone(),
            })
            .await
            .map(|_| ())
            .context(GetQueueAttributes)
            .map_err(Into::into)
    }

    pub fn create_client(&self) -> crate::Result<SqsClient> {
        let region = (&self.region).try_into()?;
        let client = rusoto::client()?;

        let creds = self.auth.build(&region, self.assume_role.clone())?;

        Ok(SqsClient::new_with(client, creds, region))
    }
}

impl SqsSink {
    pub fn new(
        config: SqsSinkConfig,
        cx: SinkContext,
        client: SqsClient,
    ) -> crate::Result<impl Sink<Event, Error = ()>> {
        // Currently we do not use batching, so this mostly for future. Also implement `Service` is simpler than `Sink`.
        // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-batch-api-actions.html
        // Up to 10 events, not more than 256KB as total size.
        let batch = BatchSettings::default().events(1).bytes(262_144);

        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding;
        let fifo = config.queue_url.ends_with(".fifo");
        let message_group_id = match (config.message_group_id, fifo) {
            (Some(value), true) => Some(Template::try_from(value).context(TopicTemplate)?),
            (Some(_), false) => return Err(Box::new(BuildError::MessageGroupIdNotAllowed)),
            (None, true) => return Err(Box::new(BuildError::MessageGroupIdMissing)),
            (None, false) => None,
        };

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
            .sink_map_err(|error| error!(message = "Fatal sqs sink error.", %error))
            .with_flat_map(move |event| {
                stream::iter(encode_event(event, &encoding, message_group_id.as_ref())).map(Ok)
            });

        Ok(sink)
    }
}

impl Service<Vec<SendMessageEntry>> for SqsSink {
    type Response = SendMessageResult;
    type Error = RusotoError<SendMessageError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut entries: Vec<SendMessageEntry>) -> Self::Future {
        assert_eq!(entries.len(), 1, "Sending batch is not supported.");

        let entry = entries.remove(0);
        let byte_size = entry.message_body.len();

        let client = self.client.clone();
        let request = SendMessageRequest {
            message_body: entry.message_body,
            message_group_id: entry.message_group_id,
            queue_url: self.queue_url.clone(),
            ..Default::default()
        };

        Box::pin(async move {
            client
                .send_message(request)
                .inspect_ok(|result| {
                    emit!(AwsSqsEventSent {
                        byte_size,
                        message_id: result.message_id.as_ref()
                    })
                })
                .instrument(info_span!("request"))
                .await
        })
    }
}

#[derive(Debug, Clone)]
struct SendMessageEntry {
    message_body: String,
    message_group_id: Option<String>,
}

impl EncodedLength for SendMessageEntry {
    fn encoded_length(&self) -> usize {
        self.message_body.len()
    }
}

impl Response for SendMessageResult {}

#[derive(Debug, Clone)]
struct SqsRetryLogic;

impl RetryLogic for SqsRetryLogic {
    type Error = RusotoError<SendMessageError>;
    type Response = SendMessageResult;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        rusoto::is_retriable_error(error)
    }
}

fn encode_event(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
    message_group_id: Option<&Template>,
) -> Option<SendMessageEntry> {
    encoding.apply_rules(&mut event);

    let message_group_id = match message_group_id {
        Some(tpl) => match tpl.render_string(&event) {
            Ok(value) => Some(value),
            Err(missing_keys) => {
                emit!(AwsSqsMessageGroupIdMissingKeys {
                    keys: &missing_keys
                });
                return None;
            }
        },
        None => None,
    };

    let log = event.into_log();
    let message_body = match encoding.codec() {
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| "".into()),
        Encoding::Json => serde_json::to_string(&log).expect("Error encoding event as json."),
    };

    Some(SendMessageEntry {
        message_body,
        message_group_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn sqs_encode_event_text() {
        let message = "hello world".to_string();
        let event = encode_event(message.clone().into(), &Encoding::Text.into(), None).unwrap();

        assert_eq!(&event.message_body, &message);
    }

    #[test]
    fn sqs_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let event = encode_event(event, &Encoding::Json.into(), None).unwrap();

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
            message_group_id: None,
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        config.clone().healthcheck(client.clone()).await.unwrap();

        let mut sink = SqsSink::new(config, cx, client.clone()).unwrap();

        let (mut input_lines, events) = random_lines_with_stream(100, 10);
        sink.send_all(&mut events.map(Ok)).await.unwrap();

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
