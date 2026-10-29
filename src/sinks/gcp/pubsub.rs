use std::{io, mem, time::Duration};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::{Bytes, BytesMut};
use http_1::{Request, Uri, header::CONTENT_TYPE};
use indoc::indoc;
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_http::empty_body;
use vector_lib::stream::batcher::BatchConfig as StreamBatchConfig;

use crate::{
    config::ValidatedSink,
    gcp::{GcpAuthConfig, GcpAuthenticator, PUBSUB_URL, Scope},
    http::client_v1::HttpClient,
    internal_events::LargeEventDroppedError,
    sinks::{
        HTTPV1RequestBuilderSnafu,
        gcs_common::config::healthcheck_status,
        prelude::*,
        util::{
            HttpEndpoint,
            encoding::Encoder as SinkEncoder,
            http::{HttpRequest, RetryStrategy},
            http_v1::{HttpService, HttpServiceRequestBuilder, http_response_retry_logic},
        },
    },
};

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Configured topic not found"))]
    TopicNotFound,
}

// 10MB maximum message size: https://cloud.google.com/pubsub/quotas#resource_limits
const MAX_BATCH_PAYLOAD_SIZE: usize = 10_000_000;

#[derive(Clone, Copy, Debug, Default)]
pub struct PubsubDefaultBatchSettings;

impl SinkBatchSettings for PubsubDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(10_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `gcp_pubsub` sink.
#[configurable_component(sink(
    "gcp_pubsub",
    "Publish observability events to GCP's Pub/Sub messaging system."
))]
#[derive(Clone, Debug)]
pub struct PubsubConfig {
    /// The project name to which to publish events.
    #[configurable(metadata(docs::examples = "vector-123456"))]
    pub project: String,

    /// The topic within the project to which to publish events.
    #[configurable(metadata(docs::examples = "this-is-a-topic"))]
    pub topic: String,

    /// The endpoint to which to publish events.
    ///
    /// The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
    /// by the [`GCP Pub/Sub`][pubsub_api] API are used.
    ///
    /// The trailing slash `/` must not be included.
    ///
    /// [pubsub_api]: https://cloud.google.com/pubsub/docs/reference/rest
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "https://us-central1-pubsub.googleapis.com"))]
    pub endpoint: HttpEndpoint,

    #[serde(default, flatten)]
    pub auth: GcpAuthConfig,

    #[serde(default)]
    pub batch: BatchConfig<PubsubDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    encoding: EncodingConfig,

    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse(PUBSUB_URL).expect("static default endpoint should be a valid http(s) URL")
}

impl GenerateConfig for PubsubConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r"
            project: my-project
            topic: my-topic
            encoding:
              codec: json
        "})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_pubsub")]
impl SinkConfig for PubsubConfig {
    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[async_trait::async_trait]
impl ValidatedSink for PubsubConfig {
    type Validated = ValidatedPubsub;

    fn validate(&self) -> crate::Result<ValidatedPubsub> {
        let uri_base = self.endpoint.append_path(&format!(
            "/v1/projects/{}/topics/{}",
            self.project, self.topic,
        ))?;

        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_PAYLOAD_SIZE)?
            .into_batcher_settings()?;
        self.encoding.validate()?;

        let transformer = self.encoding.transformer();

        Ok(ValidatedPubsub {
            uri_base,
            batch_settings,
            transformer,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedPubsub,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedPubsub {
            uri_base,
            batch_settings,
            transformer,
        } = validated.clone();

        // We only need to load the credentials if we are not targeting an emulator.
        let auth = self.auth.build(Scope::PubSub).await?;

        let serializer = self.encoding.build()?;
        let encoder = PubsubMessageEncoder {
            transformer,
            encoder: Encoder::<()>::new(serializer),
        };

        let request_settings = self.request.into_settings();
        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings.into(), cx.proxy())?;

        let healthcheck =
            healthcheck(client.clone(), uri_base.as_uri().clone(), auth.clone()).boxed();
        auth.spawn_regenerate_token();

        // The suffix is a Google API method that attaches directly to the topic path without a
        // separator.
        let service_request_builder = PubsubServiceRequestBuilder {
            uri: uri_base.append_raw_suffix(":publish")?.into_uri(),
            auth,
        };
        let service =
            HttpService::new(client, service_request_builder).with_response_decompression();
        let service = ServiceBuilder::new()
            .settings(
                request_settings,
                http_response_retry_logic(RetryStrategy::default()),
            )
            .service(service);

        let sink = PubsubSink {
            service,
            batch_settings,
            encoder,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedPubsub {
    uri_base: HttpEndpoint,
    batch_settings: BatcherSettings,
    transformer: Transformer,
}

/// One event encoded as a Pub/Sub message object.
struct PubsubMessage {
    /// The JSON object `{"data":"<base64 encoded event>"}`.
    json: String,
    finalizers: EventFinalizers,
    /// Telemetry of the event as it was received, before transformation.
    event_byte_size: usize,
    event_json_size: JsonSize,
}

impl PubsubMessage {
    /// The batch payload cost of this message: its JSON object plus one delimiter byte.
    const fn payload_size(&self) -> usize {
        self.json.len() + 1
    }
}

struct PubsubMessageEncoder {
    transformer: Transformer,
    encoder: Encoder<()>,
}

impl PubsubMessageEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<PubsubMessage> {
        // Each event is base64 encoded into the `data` item of a JSON object. Base64 output
        // never needs JSON escaping.
        const PREFIX: &str = r#"{"data":""#;
        const SUFFIX: &str = r#""}"#;

        let event_byte_size = event.size_of();
        let event_json_size = event.estimated_json_encoded_size_of();
        let finalizers = event.metadata_mut().take_finalizers();

        self.transformer.transform(&mut event);
        let mut bytes = BytesMut::new();
        // Errors are handled by `Encoder`. Dropping the finalizers leaves the event's status
        // unchanged.
        self.encoder.encode(event, &mut bytes).ok()?;

        let mut json =
            String::with_capacity(PREFIX.len() + bytes.len().div_ceil(3) * 4 + SUFFIX.len());
        json.push_str(PREFIX);
        BASE64_STANDARD.encode_string(&bytes, &mut json);
        json.push_str(SUFFIX);

        Some(PubsubMessage {
            json,
            finalizers,
            event_byte_size,
            event_json_size,
        })
    }
}

/// A batch of encoded messages.
///
/// Events dropped for being larger than the batch byte limit contribute no message, but their
/// finalizers and telemetry are accounted with the batch that was open when they were dropped.
/// Sent events are counted untagged, by their size as received.
#[derive(Default)]
struct PubsubBatch {
    messages: Vec<String>,
    finalizers: EventFinalizers,
    event_count: usize,
    events_byte_size: usize,
    events_json_size: JsonSize,
}

impl PubsubBatch {
    fn events_sent(&self) -> GroupedCountByteSize {
        CountByteSize(self.event_count, self.events_json_size).into()
    }
}

/// Batches messages by their serialized size.
///
/// A batch holds up to `item_limit` messages whose payload sizes sum to at most `size_limit`, and
/// is flushed once either limit is reached. A message that alone reaches `size_limit` is dropped.
struct PubsubBatcher {
    settings: BatcherSettings,
    batch: PubsubBatch,
    payload_size: usize,
}

impl PubsubBatcher {
    fn new(settings: BatcherSettings) -> Self {
        Self {
            settings,
            batch: PubsubBatch::default(),
            payload_size: 0,
        }
    }
}

impl StreamBatchConfig<PubsubMessage> for PubsubBatcher {
    type ItemMetadata = ();
    type Batch = PubsubBatch;

    fn len(&self) -> usize {
        self.batch.event_count
    }

    fn take_batch(&mut self) -> PubsubBatch {
        self.payload_size = 0;
        mem::take(&mut self.batch)
    }

    fn push(&mut self, message: PubsubMessage, (): ()) {
        let payload_size = self.payload_size + message.payload_size();
        self.batch.event_count += 1;
        self.batch.events_byte_size += message.event_byte_size;
        self.batch.events_json_size += message.event_json_size;
        self.batch.finalizers.merge(message.finalizers);

        if self.batch.messages.is_empty() && payload_size >= self.settings.size_limit {
            emit!(LargeEventDroppedError {
                length: message.json.len(),
                max_length: self.settings.size_limit,
            });
        } else {
            self.payload_size = payload_size;
            self.batch.messages.push(message.json);
        }
    }

    fn is_batch_full(&self) -> bool {
        self.batch.messages.len() >= self.settings.item_limit
            || self.payload_size >= self.settings.size_limit
    }

    fn item_fits_in_batch(&self, message: &PubsubMessage) -> (bool, ()) {
        let fits = self.batch.messages.is_empty()
            || (self.batch.messages.len() < self.settings.item_limit
                && self.payload_size + message.payload_size() <= self.settings.size_limit);
        (fits, ())
    }

    fn timeout(&self) -> Duration {
        self.settings.timeout
    }
}

/// Encodes a batch as the body of a `publish` request.
struct PubsubBatchEncoder;

impl SinkEncoder<PubsubBatch> for PubsubBatchEncoder {
    fn encode_input(
        &self,
        batch: PubsubBatch,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        const PREFIX: &[u8] = br#"{"messages":["#;
        const SUFFIX: &[u8] = b"]}";

        let n_events = batch.event_count;
        write_all(writer, n_events, PREFIX)?;
        let mut bytes_written = PREFIX.len();
        for (index, message) in batch.messages.iter().enumerate() {
            if index > 0 {
                write_all(writer, n_events, b",")?;
                bytes_written += 1;
            }
            write_all(writer, n_events, message.as_bytes())?;
            bytes_written += message.len();
        }
        write_all(writer, n_events, SUFFIX)?;
        bytes_written += SUFFIX.len();

        Ok((bytes_written, batch.events_sent()))
    }
}

struct PubsubRequestBuilder {
    encoder: PubsubBatchEncoder,
}

impl RequestBuilder<PubsubBatch> for PubsubRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = PubsubBatch;
    type Encoder = PubsubBatchEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut batch: PubsubBatch,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = mem::take(&mut batch.finalizers);
        let builder = RequestMetadataBuilder::new(
            batch.event_count,
            batch.events_byte_size,
            batch.events_sent(),
        );
        (finalizers, builder, batch)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), metadata, request_metadata, ())
    }
}

#[derive(Clone, Debug)]
struct PubsubServiceRequestBuilder {
    uri: Uri,
    auth: GcpAuthenticator,
}

impl HttpServiceRequestBuilder<()> for PubsubServiceRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let mut request = Request::post(self.uri.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(request.take_payload())
            .context(HTTPV1RequestBuilderSnafu)?;
        self.auth.apply(&mut request);

        Ok(request)
    }
}

struct PubsubSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    encoder: PubsubMessageEncoder,
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for PubsubSink<S>
where
    S: Service<HttpRequest<()>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let Self {
            service,
            batch_settings,
            mut encoder,
        } = *self;
        let request_builder = PubsubRequestBuilder {
            encoder: PubsubBatchEncoder,
        };

        input
            .filter_map(move |event| future::ready(encoder.encode_event(event)))
            .batched(PubsubBatcher::new(batch_settings))
            // Pub/Sub rejects a publish request without messages, so a batch that holds only
            // dropped oversized messages is rejected without a request.
            .filter_map(|batch| {
                future::ready(if batch.messages.is_empty() {
                    batch.finalizers.update_status(EventStatus::Rejected);
                    None
                } else {
                    Some(batch)
                })
            })
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| {
                future::ready(match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(request) => Some(request),
                })
            })
            .into_driver(service)
            // Like the legacy sink, requests failing after retries finalize events as `Errored`.
            .with_request_error_status(EventStatus::Errored)
            .run()
            .await
    }
}

async fn healthcheck(client: HttpClient, uri: Uri, auth: GcpAuthenticator) -> crate::Result<()> {
    let mut request = Request::get(uri).body(empty_body())?;
    auth.apply(&mut request);

    let response = client.send(request).await?;
    healthcheck_status(response.status(), HealthcheckError::TopicNotFound.into())
}

#[cfg(test)]
mod tests {
    use futures::stream;
    use indoc::indoc;
    use serde_json::json;
    use vector_lib::{
        codecs::JsonSerializerConfig,
        event::{BatchNotifier, BatchStatus, LogEvent},
    };
    use vrl::event_path;

    use super::*;
    use crate::{
        sinks::util::test::build_test_server,
        test_util::{
            addr::next_addr,
            components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        },
    };

    fn message_encoder() -> PubsubMessageEncoder {
        let encoding: EncodingConfig = JsonSerializerConfig::default().into();
        PubsubMessageEncoder {
            transformer: encoding.transformer(),
            encoder: Encoder::<()>::new(encoding.build().expect("json codec should build")),
        }
    }

    /// A log event without a timestamp, so that its encoded size is deterministic.
    fn log_event(message: &str, batch: Option<&BatchNotifier>) -> Event {
        let mut log = LogEvent::default();
        log.insert(event_path!("message"), message);
        Event::Log(match batch {
            Some(batch) => log.with_batch_notifier(batch),
            None => log,
        })
    }

    fn encode(encoder: &mut PubsubMessageEncoder, event: Event) -> PubsubMessage {
        encoder.encode_event(event).expect("event should encode")
    }

    async fn batch(
        messages: Vec<PubsubMessage>,
        size_limit: usize,
        item_limit: usize,
    ) -> Vec<PubsubBatch> {
        let settings = BatcherSettings {
            timeout: Duration::from_hours(1),
            size_limit,
            item_limit,
        };
        stream::iter(messages)
            .batched(PubsubBatcher::new(settings))
            .collect()
            .await
    }

    fn build_request(
        batch: PubsubBatch,
    ) -> (EventFinalizers, RequestMetadata, EncodeResult<Bytes>) {
        let builder = PubsubRequestBuilder {
            encoder: PubsubBatchEncoder,
        };
        let (finalizers, metadata, events) = builder.split_input(batch);
        let payload = builder
            .encode_events(events)
            .expect("encoding into memory should succeed");
        (finalizers, metadata.build(&payload), payload)
    }

    /// The legacy body: each message is `{"data": <base64>}` inside a `messages` array.
    fn expected_body(messages: &[&PubsubMessage]) -> Vec<u8> {
        let messages = messages
            .iter()
            .map(|message| serde_json::from_str(&message.json).expect("message should be JSON"))
            .collect::<Vec<serde_json::Value>>();
        serde_json::to_vec(&json!({ "messages": messages })).unwrap()
    }

    fn messages_of(batch: &PubsubBatch) -> Vec<&str> {
        batch.messages.iter().map(String::as_str).collect()
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PubsubConfig>();
    }

    #[tokio::test]
    async fn fails_missing_creds() {
        let config: PubsubConfig = serde_yaml::from_str(indoc! {r"
                project: project
                topic: topic
                encoding:
                  codec: json
            "})
        .unwrap();
        if SinkConfig::build(&config, SinkContext::default())
            .await
            .is_ok()
        {
            panic!("config.build failed to error");
        }
    }

    #[tokio::test]
    async fn publishes_transformed_base64_messages_with_api_key() {
        let (_guard, addr) = next_addr();
        let config: PubsubConfig = serde_yaml::from_str(&format!(
            indoc! {r"
                project: project
                topic: topic
                endpoint: http://{}
                api_key: YWJj
                encoding:
                  codec: json
                  except_fields: [secret]
            "},
            addr
        ))
        .unwrap();
        let (sink, _healthcheck) = SinkConfig::build(&config, SinkContext::default())
            .await
            .unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let events = ["first", "second"].map(|message| {
            let mut event = log_event(message, Some(&batch));
            event.as_mut_log().insert(event_path!("secret"), "hidden");
            event
        });
        drop(batch);
        run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (parts, body) = rx.next().await.expect("request should be received");
        assert_eq!(parts.method, http_0::Method::POST);
        assert_eq!(
            parts.uri.path(),
            "/v1/projects/project/topics/topic:publish"
        );
        assert_eq!(parts.uri.query(), Some("key=YWJj"));
        assert_eq!(parts.headers["content-type"], "application/json");

        let body_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // The body is compact JSON with nothing besides the messages envelope.
        assert_eq!(&body[..], &serde_json::to_vec(&body_json).unwrap()[..]);
        assert_eq!(body_json.as_object().unwrap().len(), 1);
        let messages = body_json["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        for (message, expected) in messages.iter().zip(["first", "second"]) {
            let message = message.as_object().unwrap();
            assert_eq!(message.len(), 1);
            let data = BASE64_STANDARD
                .decode(message["data"].as_str().unwrap())
                .unwrap();
            let event: serde_json::Value = serde_json::from_slice(&data).unwrap();
            assert_eq!(event["message"], expected);
            assert!(event.get("secret").is_none());
        }
    }

    #[tokio::test]
    async fn transport_error_marks_events_errored() {
        // Nothing listens on the reserved address, so the request fails to connect.
        let (_guard, addr) = next_addr();
        let config: PubsubConfig = serde_yaml::from_str(&format!(
            indoc! {r"
                project: project
                topic: topic
                endpoint: http://{}
                skip_authentication: true
                request:
                  retry_attempts: 0
                encoding:
                  codec: json
            "},
            addr
        ))
        .unwrap();
        let (sink, _healthcheck) = SinkConfig::build(&config, SinkContext::default())
            .await
            .unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = log_event("message", Some(&batch));
        drop(batch);
        sink.run_events(vec![event]).await.unwrap();
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));
    }

    #[tokio::test]
    async fn drops_first_message_reaching_byte_limit() {
        let mut encoder = message_encoder();
        let message = encode(&mut encoder, log_event("message", None));
        let size = message.payload_size();

        let batches = batch(vec![message], size, 1000).await;
        assert_eq!(batches.len(), 1);
        assert!(batches[0].messages.is_empty());
        assert_eq!(batches[0].event_count, 1);

        let message = encode(&mut encoder, log_event("message", None));
        let expected = message.json.clone();
        let batches = batch(vec![message], size + 1, 1000).await;
        assert_eq!(batches.len(), 1);
        assert_eq!(messages_of(&batches[0]), [expected.as_str()]);
    }

    #[tokio::test]
    async fn admits_message_filling_batch_exactly() {
        let mut encoder = message_encoder();
        let messages = ["one", "two", "six"]
            .map(|message| encode(&mut encoder, log_event(message, None)))
            .into_iter()
            .collect::<Vec<_>>();
        let size = messages[0].payload_size();
        assert!(
            messages
                .iter()
                .all(|message| message.payload_size() == size)
        );
        let jsons = messages
            .iter()
            .map(|message| message.json.clone())
            .collect::<Vec<_>>();

        let batches = batch(messages, 2 * size, 1000).await;
        assert_eq!(batches.len(), 2);
        assert_eq!(
            messages_of(&batches[0]),
            [jsons[0].as_str(), jsons[1].as_str()]
        );
        assert_eq!(messages_of(&batches[1]), [jsons[2].as_str()]);
    }

    #[tokio::test]
    async fn dropped_message_is_accounted_with_next_messages() {
        let mut encoder = message_encoder();
        let (dropped_batch, mut dropped_receiver) = BatchNotifier::new_with_receiver();
        let oversized = log_event(&"x".repeat(100), Some(&dropped_batch));
        drop(dropped_batch);
        let small = ["a", "b", "c"].map(|message| log_event(message, None));
        let json_sizes = std::iter::once(&oversized)
            .chain(&small[..2])
            .map(EstimatedJsonEncodedSizeOf::estimated_json_encoded_size_of)
            .fold(JsonSize::zero(), |total, size| total + size);

        let oversized = encode(&mut encoder, oversized);
        let size_limit = oversized.payload_size();
        let small = small.map(|event| encode(&mut encoder, event));
        let expected_first = expected_body(&[&small[0], &small[1]]);
        let expected_second = expected_body(&[&small[2]]);
        let mut messages = vec![oversized];
        messages.extend(small);

        // The dropped message does not count toward the two-message limit.
        let mut batches = batch(messages, size_limit, 2).await.into_iter();
        let (finalizers, metadata, payload) = build_request(batches.next().unwrap());
        assert_eq!(&payload.payload[..], &expected_first[..]);
        assert_eq!(metadata.event_count(), 3);
        assert_eq!(
            payload.transformed_json_size.size(),
            Some(CountByteSize(3, json_sizes))
        );

        let (_, metadata, payload) = build_request(batches.next().unwrap());
        assert_eq!(&payload.payload[..], &expected_second[..]);
        assert_eq!(metadata.event_count(), 1);
        assert!(batches.next().is_none());

        // The dropped event takes the delivery status of the request that accounts for it.
        assert!(dropped_receiver.try_recv().is_err());
        finalizers.update_status(EventStatus::Rejected);
        drop(finalizers);
        assert_eq!(dropped_receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[tokio::test]
    async fn trailing_dropped_message_is_rejected_without_request() {
        let (_guard, addr) = next_addr();
        let config: PubsubConfig = serde_yaml::from_str(&format!(
            indoc! {r"
                project: project
                topic: topic
                endpoint: http://{}
                skip_authentication: true
                encoding:
                  codec: json
                batch:
                  max_bytes: 100
            "},
            addr
        ))
        .unwrap();
        let (sink, _healthcheck) = SinkConfig::build(&config, SinkContext::default())
            .await
            .unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (small_batch, mut small_receiver) = BatchNotifier::new_with_receiver();
        let (dropped_batch, mut dropped_receiver) = BatchNotifier::new_with_receiver();
        let events = vec![
            log_event("a", Some(&small_batch)),
            log_event(&"x".repeat(100), Some(&dropped_batch)),
        ];
        drop((small_batch, dropped_batch));
        sink.run_events(events).await.unwrap();

        assert_eq!(small_receiver.try_recv(), Ok(BatchStatus::Delivered));
        assert_eq!(dropped_receiver.try_recv(), Ok(BatchStatus::Rejected));
        let (_, body) = rx.next().await.expect("request should be received");
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert!(
            rx.try_recv().is_err(),
            "no request should publish the dropped message"
        );
    }
}

#[cfg(all(test, feature = "gcp-integration-tests"))]
mod integration_tests {
    use reqwest::{Client, Method, Response};
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use vector_lib::{
        codecs::JsonSerializerConfig,
        event::{BatchNotifier, BatchStatus},
    };

    use super::*;
    use crate::{
        gcp,
        test_util::{
            components::{
                COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS, run_and_assert_sink_compliance,
                run_and_assert_sink_error,
            },
            random_events_with_stream, random_metrics_with_stream, random_string, trace_init,
        },
    };

    const PROJECT: &str = "testproject";

    fn config(topic: &str) -> PubsubConfig {
        PubsubConfig {
            project: PROJECT.into(),
            topic: topic.into(),
            endpoint: HttpEndpoint::parse(&gcp::PUBSUB_ADDRESS).unwrap(),
            auth: GcpAuthConfig {
                skip_authentication: true,
                ..Default::default()
            },
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            encoding: JsonSerializerConfig::default().into(),
            tls: Option::default(),
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }

    async fn config_build(topic: &str) -> (VectorSink, crate::sinks::Healthcheck) {
        let cx = SinkContext::default();
        SinkConfig::build(&config(topic), cx)
            .await
            .expect("Building sink failed")
    }

    #[tokio::test]
    async fn publish_metrics() {
        trace_init();

        let (topic, subscription) = create_topic_subscription().await;
        let (sink, healthcheck) = config_build(&topic).await;

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_metrics_with_stream(100, Some(batch), None);
        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let response = pull_messages(&subscription, 1000).await;
        let messages = response
            .receivedMessages
            .as_ref()
            .expect("Response is missing messages");
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = messages[i].message.decode_data_as_value();
            let data = serde_json::to_value(data).unwrap();
            let expected = serde_json::to_value(input[i].as_metric()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[tokio::test]
    async fn publish_events() {
        trace_init();

        let (topic, subscription) = create_topic_subscription().await;
        let (sink, healthcheck) = config_build(&topic).await;

        healthcheck.await.expect("Health check failed");

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let response = pull_messages(&subscription, 1000).await;
        let messages = response
            .receivedMessages
            .as_ref()
            .expect("Response is missing messages");
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = messages[i].message.decode_data();
            let data = serde_json::to_value(data).unwrap();
            let expected =
                serde_json::to_value(input[i].as_log().all_event_fields().unwrap()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[tokio::test]
    async fn publish_events_broken_topic() {
        trace_init();

        let (topic, _subscription) = create_topic_subscription().await;
        let (sink, _healthcheck) = config_build(&format!("BREAK{topic}BREAK")).await;
        // Explicitly skip healthcheck

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input, events) = random_events_with_stream(100, 100, Some(batch));
        run_and_assert_sink_error(sink, events, &COMPONENT_ERROR_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[tokio::test]
    async fn checks_for_valid_topic() {
        trace_init();

        let (topic, _subscription) = create_topic_subscription().await;
        let topic = format!("BAD{topic}");
        let (_sink, healthcheck) = config_build(&topic).await;
        healthcheck.await.expect_err("Health check did not fail");
    }

    async fn create_topic_subscription() -> (String, String) {
        let topic = format!("topic-{}", random_string(10));
        let subscription = format!("subscription-{}", random_string(10));
        request(Method::PUT, &format!("topics/{topic}"), json!({}))
            .await
            .json::<Value>()
            .await
            .expect("Creating new topic failed");
        request(
            Method::PUT,
            &format!("subscriptions/{subscription}"),
            json!({ "topic": format!("projects/{PROJECT}/topics/{topic}") }),
        )
        .await
        .json::<Value>()
        .await
        .expect("Creating new subscription failed");
        (topic, subscription)
    }

    async fn request(method: Method, path: &str, json: Value) -> Response {
        let url = format!("{}/v1/projects/{PROJECT}/{path}", *gcp::PUBSUB_ADDRESS);
        Client::new()
            .request(method.clone(), &url)
            .json(&json)
            .send()
            .await
            .unwrap_or_else(|_| panic!("Sending {method} request to {url} failed"))
    }

    async fn pull_messages(subscription: &str, count: usize) -> PullResponse {
        request(
            Method::POST,
            &format!("subscriptions/{subscription}:pull"),
            json!({
                "returnImmediately": true,
                "maxMessages": count
            }),
        )
        .await
        .json::<PullResponse>()
        .await
        .expect("Extracting pull data failed")
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullResponse {
        receivedMessages: Option<Vec<PullMessageOuter>>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    #[allow(dead_code)] // deserialize all fields
    struct PullMessageOuter {
        ackId: String,
        message: PullMessage,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    #[allow(dead_code)] // deserialize all fields
    struct PullMessage {
        data: String,
        messageId: String,
        publishTime: String,
    }

    impl PullMessage {
        fn decode_data(&self) -> TestMessage {
            let data = BASE64_STANDARD
                .decode(&self.data)
                .expect("Invalid base64 data");
            let data = String::from_utf8_lossy(&data);
            serde_json::from_str(&data).expect("Invalid message structure")
        }

        fn decode_data_as_value(&self) -> Value {
            let data = BASE64_STANDARD
                .decode(&self.data)
                .expect("Invalid base64 data");
            let data = String::from_utf8_lossy(&data);
            serde_json::from_str(&data).expect("Invalid json")
        }
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct TestMessage {
        timestamp: String,
        message: String,
    }
}
