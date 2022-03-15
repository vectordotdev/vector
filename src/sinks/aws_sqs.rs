use aws_sdk_sqs::error::{GetQueueAttributesError, SendMessageError};
use aws_sdk_sqs::output::SendMessageOutput;
use aws_sdk_sqs::types::SdkError;
use aws_sdk_sqs::Client as SqsClient;

use std::{
    convert::TryFrom,
    num::NonZeroU64,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;
use crate::aws::aws_sdk::{create_client, is_retriable_error};
use crate::aws::{AwsAuthentication, RegionOrEndpoint};
use crate::common::sqs::SqsClientBuilder;
use crate::{
    config::{
        log_schema, AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext, SinkDescription,
    },
    event::Event,
    internal_events::{AwsSqsEventsSent, TemplateRenderingError},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::Response,
        BatchConfig, EncodedEvent, EncodedLength, TowerRequestConfig, VecBuffer,
    },
    template::{Template, TemplateParseError},
    tls::TlsOptions,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("`message_group_id` should be defined for FIFO queue."))]
    MessageGroupIdMissing,
    #[snafu(display("`message_group_id` is not allowed with non-FIFO queue."))]
    MessageGroupIdNotAllowed,
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
    #[snafu(display("invalid message_deduplication_id template: {}", source))]
    MessageDeduplicationIdTemplate { source: TemplateParseError },
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("GetQueueAttributes failed: {}", source))]
    GetQueueAttributes {
        source: SdkError<GetQueueAttributesError>,
    },
}

#[derive(Clone)]
pub struct SqsSink {
    client: SqsClient,
    queue_url: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SqsSinkDefaultBatchSettings;

impl SinkBatchSettings for SqsSinkDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = Some(262_144);
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SqsSinkConfig {
    pub queue_url: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    pub message_group_id: Option<String>,
    pub message_deduplication_id: Option<String>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
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
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone());
        let sink = SqsSink::new(self.clone(), cx, client)?;
        Ok((
            super::VectorSink::from_event_sink(sink),
            healthcheck.boxed(),
        ))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "aws_sqs"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

impl SqsSinkConfig {
    pub async fn healthcheck(self, client: SqsClient) -> crate::Result<()> {
        client
            .get_queue_attributes()
            .queue_url(self.queue_url.clone())
            .send()
            .await
            .map(|_| ())
            .context(GetQueueAttributesSnafu)
            .map_err(Into::into)
    }

    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SqsClient> {
        create_client::<SqsClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            proxy,
            &self.tls,
        )
        .await
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
        let batch: BatchConfig<SqsSinkDefaultBatchSettings> = BatchConfig::default();
        let batch_settings = batch.into_batch_settings()?;

        let request = config.request.unwrap_with(&TowerRequestConfig {
            timeout_secs: Some(30),
            ..Default::default()
        });
        let encoding = config.encoding;
        let fifo = config.queue_url.ends_with(".fifo");
        let message_group_id = match (config.message_group_id, fifo) {
            (Some(value), true) => Some(Template::try_from(value).context(TopicTemplateSnafu)?),
            (Some(_), false) => return Err(Box::new(BuildError::MessageGroupIdNotAllowed)),
            (None, true) => return Err(Box::new(BuildError::MessageGroupIdMissing)),
            (None, false) => None,
        };
        let message_deduplication_id = config
            .message_deduplication_id
            .map(Template::try_from)
            .transpose()?;

        let sqs = SqsSink {
            client,
            queue_url: config.queue_url,
        };

        let sink = request
            .batch_sink(
                SqsRetryLogic,
                sqs,
                VecBuffer::new(batch_settings.size),
                batch_settings.timeout,
                cx.acker(),
            )
            .sink_map_err(|error| error!(message = "Fatal sqs sink error.", %error))
            .with_flat_map(move |event| {
                stream::iter(encode_event(
                    event,
                    &encoding,
                    &message_group_id,
                    &message_deduplication_id,
                ))
                .map(Ok)
            });

        Ok(sink)
    }
}

impl Service<Vec<SendMessageEntry>> for SqsSink {
    type Response = SendMessageOutput;
    type Error = SdkError<SendMessageError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut entries: Vec<SendMessageEntry>) -> Self::Future {
        assert_eq!(entries.len(), 1, "Sending batch is not supported.");

        let entry = entries.remove(0);
        let byte_size = entry.message_body.len();

        let client = self.client.clone();
        let queue_url = self.queue_url.clone();
        Box::pin(async move {
            client
                .send_message()
                .message_body(entry.message_body)
                .set_message_group_id(entry.message_group_id)
                .set_message_deduplication_id(entry.message_deduplication_id)
                .queue_url(queue_url)
                .send()
                .inspect_ok(|result| {
                    emit!(&AwsSqsEventsSent {
                        byte_size,
                        message_id: result.message_id.as_ref()
                    })
                })
                .instrument(info_span!("request").or_current())
                .await
        })
    }
}

#[derive(Debug, Clone)]
struct SendMessageEntry {
    message_body: String,
    message_group_id: Option<String>,
    message_deduplication_id: Option<String>,
}

impl EncodedLength for SendMessageEntry {
    fn encoded_length(&self) -> usize {
        self.message_body.len()
    }
}

impl Response for SendMessageOutput {}

#[derive(Debug, Clone)]
struct SqsRetryLogic;

impl RetryLogic for SqsRetryLogic {
    type Error = SdkError<SendMessageError>;
    type Response = SendMessageOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        is_retriable_error(error)
    }
}

fn encode_event(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
    message_group_id: &Option<Template>,
    message_deduplication_id: &Option<Template>,
) -> Option<EncodedEvent<SendMessageEntry>> {
    let byte_size = event.size_of();
    encoding.apply_rules(&mut event);

    let message_group_id = match message_group_id {
        Some(tpl) => match tpl.render_string(&event) {
            Ok(value) => Some(value),
            Err(error) => {
                emit!(&TemplateRenderingError {
                    error,
                    field: Some("message_group_id"),
                    drop_event: true
                });
                return None;
            }
        },
        None => None,
    };
    let message_deduplication_id = match message_deduplication_id {
        Some(tpl) => match tpl.render_string(&event) {
            Ok(value) => Some(value),
            Err(error) => {
                emit!(&TemplateRenderingError {
                    error,
                    field: Some("message_deduplication_id"),
                    drop_event: true
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

    Some(EncodedEvent::new(
        SendMessageEntry {
            message_body,
            message_group_id,
            message_deduplication_id,
        },
        byte_size,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn sqs_encode_event_text() {
        let message = "hello world".to_string();
        let event =
            encode_event(message.clone().into(), &Encoding::Text.into(), &None, &None).unwrap();

        assert_eq!(&event.item.message_body, &message);
    }

    #[test]
    fn sqs_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let event = encode_event(event, &Encoding::Json.into(), &None, &None).unwrap();

        let map: BTreeMap<String, String> = serde_json::from_str(&event.item.message_body).unwrap();

        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn sqs_encode_event_deduplication_id() {
        let message_deduplication_id = Template::try_from("{{ transaction_id }}").unwrap();
        let mut log = LogEvent::from("hello world".to_string());
        log.insert("transaction_id", "some id");
        let event = encode_event(
            log.into(),
            &Encoding::Json.into(),
            &None,
            &Some(message_deduplication_id),
        )
        .unwrap();

        assert_eq!(
            event.item.message_deduplication_id,
            Some("some id".to_string())
        );
    }
}

#[cfg(feature = "aws-sqs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    #![allow(clippy::print_stdout)] //tests

    use aws_sdk_sqs::model::QueueAttributeName;
    use aws_sdk_sqs::{Endpoint, Region};
    use http::Uri;
    use std::collections::HashMap;
    use std::str::FromStr;
    use tokio::time::{sleep, Duration};

    use super::*;
    use crate::sinks::VectorSink;
    use crate::test_util::{random_lines_with_stream, random_string};

    fn sqs_address() -> String {
        std::env::var("SQS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
    }

    async fn create_test_client() -> SqsClient {
        let auth = AwsAuthentication::test_auth();

        let endpoint = sqs_address();
        let proxy = ProxyConfig::default();
        create_client::<SqsClientBuilder>(
            &auth,
            Some(Region::new("localstack")),
            Some(Endpoint::immutable(Uri::from_str(&endpoint).unwrap())),
            &proxy,
            &None,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn sqs_send_message_batch() {
        let cx = SinkContext::new_test();

        let queue_name = gen_queue_name();
        ensure_queue(queue_name.clone()).await;
        let queue_url = get_queue_url(queue_name.clone()).await;

        let client = create_test_client().await;

        let config = SqsSinkConfig {
            queue_url: queue_url.clone(),
            region: RegionOrEndpoint::with_endpoint(sqs_address().as_str()),
            encoding: Encoding::Text.into(),
            message_group_id: None,
            message_deduplication_id: None,
            request: Default::default(),
            tls: Default::default(),
            assume_role: None,
            auth: Default::default(),
            acknowledgements: Default::default(),
        };

        config.clone().healthcheck(client.clone()).await.unwrap();

        let sink = SqsSink::new(config, cx, client.clone()).unwrap();
        let sink = VectorSink::from_event_sink(sink);

        let (mut input_lines, events) = random_lines_with_stream(100, 10, None);
        sink.run(events).await.unwrap();

        sleep(Duration::from_secs(1)).await;

        let response = client
            .receive_message()
            .max_number_of_messages(input_lines.len() as i32)
            .queue_url(queue_url)
            .send()
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

    async fn ensure_queue(queue_name: String) {
        let client = create_test_client().await;

        let attributes: Option<HashMap<QueueAttributeName, String>> =
            if queue_name.ends_with(".fifo") {
                let mut hash_map = HashMap::new();
                hash_map.insert(QueueAttributeName::FifoQueue, "true".into());
                Some(hash_map)
            } else {
                None
            };

        if let Err(error) = client
            .create_queue()
            .set_attributes(attributes)
            .queue_name(queue_name)
            .send()
            .await
        {
            println!("Unable to check the queue {:?}", error);
        }
    }

    async fn get_queue_url(queue_name: String) -> String {
        let client = create_test_client().await;

        client
            .get_queue_url()
            .queue_name(queue_name)
            .send()
            .await
            .unwrap()
            .queue_url
            .unwrap()
    }

    fn gen_queue_name() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
