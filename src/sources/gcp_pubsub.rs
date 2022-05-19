use std::{error::Error as _, pin::Pin, sync::Arc, time::Duration};

use chrono::{DateTime, NaiveDateTime, Utc};
use codecs::decoding::{DeserializerConfig, FramingConfig};
use derivative::Derivative;
use futures::{stream, Stream, StreamExt, TryFutureExt};
use http::uri::{InvalidUri, Scheme, Uri};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio::sync::Mutex;
use tonic::{
    metadata::{errors::InvalidMetadataValue, MetadataValue},
    transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity},
    Code, Request, Status,
};
use vector_core::ByteSizeOf;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{AcknowledgementsConfig, DataType, Output, SourceConfig, SourceContext},
    event::{BatchNotifier, BatchStatus, Event, MaybeAsLogMut, Value},
    gcp::{GcpAuthConfig, GcpCredentials, Scope, PUBSUB_URL},
    internal_events::{
        BytesReceived, GcpPubsubConnectError, GcpPubsubReceiveError, GcpPubsubStreamingPullError,
        StreamClosedError,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util,
    sources::util::finalizer::{EmptyStream, UnorderedFinalizer},
    tls::{TlsConfig, TlsSettings},
    SourceSender,
};

const MIN_ACK_DEADLINE_SECONDS: i32 = 10;
const MAX_ACK_DEADLINE_SECONDS: i32 = 600;

type Finalizer = UnorderedFinalizer<Vec<String>>;

// prost emits some generated code that includes clones on `Arc`
// objects, which causes a clippy ding on this block. We don't
// directly control the generated code, so allow this lint here.
#[allow(clippy::clone_on_ref_ptr)]
mod proto {
    include!(concat!(env!("OUT_DIR"), "/google.pubsub.v1.rs"));

    use vector_core::ByteSizeOf;

    impl ByteSizeOf for StreamingPullResponse {
        fn allocated_bytes(&self) -> usize {
            self.received_messages.size_of()
        }
    }

    impl ByteSizeOf for ReceivedMessage {
        fn allocated_bytes(&self) -> usize {
            self.ack_id.size_of() + self.message.as_ref().map_or(0, ByteSizeOf::size_of)
        }
    }

    impl ByteSizeOf for PubsubMessage {
        fn allocated_bytes(&self) -> usize {
            self.data.len()
                + self.message_id.len()
                + self.ordering_key.len()
                + self.attributes.size_of()
        }
    }
}

#[derive(Debug, Snafu)]
pub(crate) enum PubsubError {
    #[snafu(display("Could not parse credentials metadata: {}", source))]
    Metadata { source: InvalidMetadataValue },
    #[snafu(display("Invalid endpoint URI: {}", source))]
    Uri { source: InvalidUri },
    #[snafu(display("Could not create endpoint: {}", source))]
    Endpoint { source: InvalidUri },
    #[snafu(display("Could not set up endpoint TLS settings: {}", source))]
    EndpointTls { source: tonic::transport::Error },
    #[snafu(display("Could not connect: {}", source))]
    Connect { source: tonic::transport::Error },
    #[snafu(display("Could not pull data from remote: {}", source))]
    Pull { source: Status },
    #[snafu(display(
        "`ack_deadline_seconds` is outside the valid range of {} to {}",
        MIN_ACK_DEADLINE_SECONDS,
        MAX_ACK_DEADLINE_SECONDS
    ))]
    InvalidAckDeadline,
}

static CLIENT_ID: Lazy<String> = Lazy::new(|| uuid::Uuid::new_v4().to_string());

#[derive(Deserialize, Serialize, Derivative, Debug, Clone)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    pub project: String,
    pub subscription: String,
    pub endpoint: Option<String>,

    #[serde(default)]
    pub skip_authentication: bool,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    pub tls: Option<TlsConfig>,

    #[serde(default = "default_ack_deadline")]
    pub ack_deadline_seconds: i32,

    #[serde(default = "default_retry_delay")]
    pub retry_delay_seconds: f64,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: AcknowledgementsConfig,
}

const fn default_ack_deadline() -> i32 {
    600
}

const fn default_retry_delay() -> f64 {
    1.0
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_pubsub")]
impl SourceConfig for PubsubConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        if self.ack_deadline_seconds < MIN_ACK_DEADLINE_SECONDS
            || self.ack_deadline_seconds > MAX_ACK_DEADLINE_SECONDS
        {
            return Err(PubsubError::InvalidAckDeadline.into());
        }

        let credentials = if self.skip_authentication {
            None
        } else {
            self.auth.make_credentials(Scope::PubSub).await?
        };

        let endpoint = self.endpoint.as_deref().unwrap_or(PUBSUB_URL).to_string();
        let uri: Uri = endpoint.parse().context(UriSnafu)?;
        let source = PubsubSource {
            endpoint,
            uri,
            credentials,
            subscription: format!(
                "projects/{}/subscriptions/{}",
                self.project, self.subscription
            ),
            decoder: DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build(),
            acknowledgements: cx.do_acknowledgements(&self.acknowledgements),
            tls: TlsSettings::from_options(&self.tls)?,
            shutdown: cx.shutdown,
            out: cx.out,
            ack_deadline_seconds: self.ack_deadline_seconds,
            ack_ids: Default::default(),
            retry_delay: Duration::from_secs_f64(self.retry_delay_seconds),
        }
        .run()
        .map_err(|error| error!(message = "Source failed.", %error));
        Ok(Box::pin(source))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "gcp_pubsub"
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl_generate_config_from_default!(PubsubConfig);

struct PubsubSource {
    endpoint: String,
    uri: Uri,
    credentials: Option<GcpCredentials>,
    subscription: String,
    decoder: Decoder,
    acknowledgements: bool,
    tls: TlsSettings,
    ack_deadline_seconds: i32,
    shutdown: ShutdownSignal,
    out: SourceSender,
    // The acknowledgement IDs are pulled out of the response message
    // and then inserted into the request. However, the request is
    // generated in a separate async task from the response handling,
    // so the data needs to be shared this way.
    ack_ids: Arc<Mutex<Vec<String>>>,
    retry_delay: Duration,
}

enum State {
    RetryNow,
    RetryDelay,
    Shutdown,
}

impl PubsubSource {
    async fn run(mut self) -> crate::Result<()> {
        let mut endpoint = Channel::from_shared(self.endpoint.clone()).context(EndpointSnafu)?;
        if self.uri.scheme() != Some(&Scheme::HTTP) {
            endpoint = endpoint
                .tls_config(self.make_tls_config())
                .context(EndpointTlsSnafu)?;
        }

        let mut token_generator = match &self.credentials {
            Some(credentials) => credentials.clone().token_regenerator().boxed(),
            None => EmptyStream::default().boxed(),
        };

        loop {
            match self.run_once(&endpoint, &mut token_generator).await {
                State::RetryNow => debug!("Retrying immediately."),
                State::RetryDelay => {
                    info!(
                        timeout_secs = self.retry_delay.as_secs_f64(),
                        "Retrying after timeout."
                    );
                    tokio::time::sleep(self.retry_delay).await;
                }
                State::Shutdown => break,
            }
        }

        Ok(())
    }

    async fn run_once(
        &mut self,
        endpoint: &Endpoint,
        token_generator: &mut Pin<Box<dyn Stream<Item = ()> + Send>>,
    ) -> State {
        let connection = match endpoint.connect().await {
            Ok(connection) => connection,
            Err(error) => {
                emit!(GcpPubsubConnectError { error });
                return State::RetryDelay;
            }
        };

        let mut client = proto::subscriber_client::SubscriberClient::with_interceptor(
            connection,
            |mut req: Request<()>| {
                if let Some(credentials) = &self.credentials {
                    let authorization = MetadataValue::try_from(&credentials.make_token())
                        .map_err(|_| {
                            Status::new(
                                Code::FailedPrecondition,
                                "Invalid token text returned by GCP",
                            )
                        })?;
                    req.metadata_mut().insert("authorization", authorization);
                }
                Ok(req)
            },
        );

        // Handle shutdown during startup, the streaming pull doesn't
        // start if there is no data in the subscription.
        let request_stream = self.request_stream();
        debug!("Starting streaming pull.");
        let stream = tokio::select! {
            _ = &mut self.shutdown => return State::Shutdown,
            result = client.streaming_pull(request_stream) => match result {
                Ok(stream) => stream,
                Err(error) => {
                    emit!(GcpPubsubStreamingPullError { error });
                    return State::RetryDelay;
                }
            }
        };
        let mut stream = stream.into_inner();

        let (finalizer, mut ack_stream) =
            Finalizer::maybe_new(self.acknowledgements, self.shutdown.clone());

        loop {
            tokio::select! {
                _ = &mut self.shutdown => return State::Shutdown,
                _ = &mut token_generator.next() => {
                    debug!("New authentication token generated, restarting stream.");
                    break State::RetryNow;
                },
                receipts = ack_stream.next() => if let Some((status, receipts)) = receipts {
                    if status == BatchStatus::Delivered {
                        self.ack_ids.lock().await.extend(receipts);
                    }
                },
                response = stream.next() => match response {
                    Some(Ok(response)) => self.handle_response(response, &finalizer).await,
                    Some(Err(error)) => break translate_error(error),
                    None => break State::RetryNow,
                },
            }
        }
    }

    fn make_tls_config(&self) -> ClientTlsConfig {
        let host = self.uri.host().unwrap_or("pubsub.googleapis.com");
        let mut config = ClientTlsConfig::new().domain_name(host);
        if let Some((cert, key)) = self.tls.identity_pem() {
            config = config.identity(Identity::from_pem(cert, key));
        }
        for authority in self.tls.authorities_pem() {
            config = config.ca_certificate(Certificate::from_pem(authority));
        }
        config
    }

    fn request_stream(&self) -> impl Stream<Item = proto::StreamingPullRequest> + 'static {
        // This data is only allowed in the first request
        let mut subscription = Some(self.subscription.clone());
        let mut client_id = Some(CLIENT_ID.clone());

        let ack_ids = Arc::clone(&self.ack_ids);
        let stream_ack_deadline_seconds = self.ack_deadline_seconds;
        stream::repeat(()).then(move |()| {
            let ack_ids = Arc::clone(&ack_ids);
            let subscription = subscription.take().unwrap_or_default();
            let client_id = client_id.take().unwrap_or_default();
            async move {
                let mut ack_ids = ack_ids.lock().await;
                proto::StreamingPullRequest {
                    subscription,
                    client_id,
                    ack_ids: std::mem::take(ack_ids.as_mut()),
                    stream_ack_deadline_seconds,
                    ..Default::default()
                }
            }
        })
    }

    async fn handle_response(
        &mut self,
        response: proto::StreamingPullResponse,
        finalizer: &Option<Finalizer>,
    ) {
        emit!(BytesReceived {
            byte_size: response.size_of(),
            protocol: self.uri.scheme().map(Scheme::as_str).unwrap_or("http"),
        });

        let (batch, notifier) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
        let (events, ids) = self.parse_messages(response.received_messages, batch).await;

        let count = events.len();
        match self.out.send_batch(events).await {
            Err(error) => emit!(StreamClosedError { error, count }),
            Ok(()) => match notifier {
                None => self.ack_ids.lock().await.extend(ids),
                Some(notifier) => finalizer
                    .as_ref()
                    .expect("Finalizer must have been set up for acknowledgements")
                    .add(ids, notifier),
            },
        }
    }

    async fn parse_messages(
        &self,
        response: Vec<proto::ReceivedMessage>,
        batch: Option<Arc<BatchNotifier>>,
    ) -> (Vec<Event>, Vec<String>) {
        let mut ack_ids = Vec::with_capacity(response.len());
        let events = response
            .into_iter()
            .flat_map(|received| {
                ack_ids.push(received.ack_id);
                received
                    .message
                    .map(|message| self.parse_message(message, &batch))
            })
            .flatten()
            .collect();
        (events, ack_ids)
    }

    fn parse_message<'a>(
        &self,
        message: proto::PubsubMessage,
        batch: &'a Option<Arc<BatchNotifier>>,
    ) -> impl Iterator<Item = Event> + 'a {
        let attributes = Value::Object(
            message
                .attributes
                .into_iter()
                .map(|(key, value)| (key, Value::Bytes(value.into())))
                .collect(),
        );
        util::decode_message(
            self.decoder.clone(),
            "gcp_pubsub",
            &message.data,
            message.publish_time.map(|dt| {
                DateTime::from_utc(
                    NaiveDateTime::from_timestamp(dt.seconds, dt.nanos as u32),
                    Utc,
                )
            }),
            batch,
        )
        .map(move |mut event| {
            if let Some(log) = event.maybe_as_log_mut() {
                log.insert("message_id", message.message_id.clone());
                log.insert("attributes", attributes.clone());
            }
            event
        })
    }
}

fn translate_error(error: tonic::Status) -> State {
    // GCP occasionally issues a connection reset
    // in the middle of the streaming pull. This
    // reset is not technically an error, so we
    // want to retry immediately, but it is
    // reported to us as an error from the
    // underlying library (`tonic`).
    if is_reset(&error) {
        debug!("Stream reset by server.");
        State::RetryNow
    } else {
        emit!(GcpPubsubReceiveError { error });
        State::RetryDelay
    }
}

fn is_reset(error: &Status) -> bool {
    error
        .source()
        .and_then(|source| source.downcast_ref::<hyper::Error>())
        .and_then(|error| error.source())
        .and_then(|source| source.downcast_ref::<h2::Error>())
        .map_or(false, |error| error.is_remote() && error.is_reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PubsubConfig>();
    }
}

#[cfg(all(test, feature = "gcp-pubsub-integration-tests"))]
mod integration_tests {
    use std::collections::{BTreeMap, HashSet};

    use futures::{Stream, StreamExt};
    use http::method::Method;
    use hyper::{Request, StatusCode};
    use once_cell::sync::Lazy;
    use serde_json::{json, Value};
    use tokio::time::{Duration, Instant};
    use vector_common::btreemap;

    use super::*;
    use crate::config::{ComponentKey, ProxyConfig};
    use crate::test_util::components::{assert_source_compliance, SOURCE_TAGS};
    use crate::test_util::{self, components, random_string};
    use crate::{event::EventStatus, gcp, http::HttpClient, shutdown, SourceSender};

    const PROJECT: &str = "sourceproject";
    static PROJECT_URI: Lazy<String> =
        Lazy::new(|| format!("{}/v1/projects/{}", *gcp::PUBSUB_ADDRESS, PROJECT));
    const ACK_DEADLINE: u64 = 10; // Minimum custom deadline allowed by Pub/Sub

    #[tokio::test]
    async fn oneshot() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;
            let test_data = tester.send_test_events(99, btreemap![]).await;
            receive_events(&mut rx, test_data).await;
            tester.shutdown_check(shutdown).await;
        })
        .await;
    }

    #[tokio::test]
    async fn shuts_down_before_data_received() {
        let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;

        tester.shutdown(shutdown).await; // Not shutdown_check because this emits nothing

        assert!(rx.next().await.is_none());
        tester.send_test_events(1, btreemap![]).await;
        assert!(rx.next().await.is_none());
        assert_eq!(tester.pull_count(1).await, 1);
    }

    #[tokio::test]
    async fn shuts_down_after_data_received() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;

            let test_data = tester.send_test_events(1, btreemap![]).await;
            receive_events(&mut rx, test_data).await;

            tester.shutdown_check(shutdown).await;

            assert!(rx.next().await.is_none());
            tester.send_test_events(1, btreemap![]).await;
            assert!(rx.next().await.is_none());
            // The following assert is there to test that the source isn't
            // pulling anything out of the subscription after it reports
            // shutdown. It works when there wasn't anything previously in
            // the topic, but does not work here despite evidence that the
            // entire tokio task has exited.
            // assert_eq!(tester.pull_count(1).await, 1);
        })
        .await;
    }

    #[tokio::test]
    async fn streams_data() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;
            for _ in 0..10 {
                let test_data = tester.send_test_events(9, btreemap![]).await;
                receive_events(&mut rx, test_data).await;
            }
            tester.shutdown_check(shutdown).await;
        })
        .await;
    }

    #[tokio::test]
    async fn sends_attributes() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;
            let attributes = btreemap![
                random_string(8) => random_string(88),
                random_string(8) => random_string(88),
                random_string(8) => random_string(88),
            ];
            let test_data = tester.send_test_events(1, attributes).await;
            receive_events(&mut rx, test_data).await;
            tester.shutdown_check(shutdown).await;
        })
        .await;
    }

    #[tokio::test]
    async fn acks_received() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;

            let test_data = tester.send_test_events(1, btreemap![]).await;
            receive_events(&mut rx, test_data).await;

            tester.shutdown_check(shutdown).await;

            // Make sure there are no messages left in the queue
            assert_eq!(tester.pull_count(10).await, 0);

            // Wait for the acknowledgement deadline to expire
            tokio::time::sleep(Duration::from_secs(ACK_DEADLINE + 1)).await;

            // All messages are still acknowledged
            assert_eq!(tester.pull_count(10).await, 0);
        })
        .await;
    }

    #[tokio::test]
    // I have verified manually that the streaming code above omits the
    // acknowledgements when events are rejected, but have been unable
    // to verify the events are not acknowledged through the emulator.
    #[ignore]
    async fn does_not_ack_rejected() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Rejected).await;

            let test_data = tester.send_test_events(1, btreemap![]).await;
            receive_events(&mut rx, test_data).await;

            tester.shutdown(shutdown).await;

            // Make sure there are no messages left in the queue
            assert_eq!(tester.pull_count(10).await, 0);

            // Wait for the acknowledgement deadline to expire
            tokio::time::sleep(std::time::Duration::from_secs(ACK_DEADLINE + 1)).await;

            // All messages are still in the queue
            assert_eq!(tester.pull_count(10).await, 1);
        })
        .await;
    }

    async fn setup(
        status: EventStatus,
    ) -> (
        Tester,
        impl Stream<Item = Event> + Unpin,
        shutdown::SourceShutdownCoordinator,
    ) {
        components::init_test();

        let tls_settings = TlsSettings::from_options(&None).unwrap();
        let client = HttpClient::new(tls_settings, &ProxyConfig::default()).unwrap();
        let tester = Tester::new(client).await;

        let (rx, shutdown) = tester.spawn_source(status).await;

        (tester, rx, shutdown)
    }

    fn now_trunc() -> DateTime<Utc> {
        let start = Utc::now().timestamp();
        // Truncate the milliseconds portion, the hard way.
        DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(start, 0), Utc)
    }

    struct Tester {
        client: HttpClient,
        topic: String,
        subscription: String,
        component: ComponentKey,
    }

    struct TestData {
        lines: Vec<String>,
        start: DateTime<Utc>,
        attributes: BTreeMap<String, String>,
    }

    impl Tester {
        async fn new(client: HttpClient) -> Self {
            let this = Self {
                client,
                topic: format!("topic-{}", random_string(10).to_lowercase()),
                subscription: format!("sub-{}", random_string(10).to_lowercase()),
                component: ComponentKey::from("gcp_pubsub"),
            };

            this.request(Method::PUT, "topics/{topic}", json!({})).await;

            let body = json!({
                "topic": format!("projects/{}/topics/{}", PROJECT, this.topic),
                "ackDeadlineSeconds": ACK_DEADLINE,
            });
            this.request(Method::PUT, "subscriptions/{sub}", body).await;

            this
        }

        async fn spawn_source(
            &self,
            status: EventStatus,
        ) -> (
            impl Stream<Item = Event> + Unpin,
            shutdown::SourceShutdownCoordinator,
        ) {
            let (tx, rx) = SourceSender::new_test_finalize(status);
            let config = PubsubConfig {
                project: PROJECT.into(),
                subscription: self.subscription.clone(),
                endpoint: Some(gcp::PUBSUB_ADDRESS.clone()),
                skip_authentication: true,
                ack_deadline_seconds: ACK_DEADLINE as i32,
                ..Default::default()
            };
            let (mut ctx, shutdown) = SourceContext::new_shutdown(&self.component, tx);
            ctx.acknowledgements = true;
            let source = config.build(ctx).await.expect("Failed to build source");
            tokio::spawn(async move { source.await.expect("Failed to run source") });

            (rx, shutdown)
        }

        async fn send_test_events(
            &self,
            count: usize,
            attributes: BTreeMap<String, String>,
        ) -> TestData {
            let start = now_trunc();
            let lines: Vec<_> = test_util::random_lines(44).take(count).collect();
            let messages: Vec<_> = lines
                .iter()
                .map(|message| base64::encode(&message))
                .map(|data| json!({ "data": data, "attributes": attributes.clone() }))
                .collect();
            let body = json!({ "messages": messages });
            self.request(Method::POST, "topics/{topic}:publish", body)
                .await;

            TestData {
                lines,
                start,
                attributes,
            }
        }

        async fn pull_count(&self, count: usize) -> usize {
            let response = self
                .request(
                    Method::POST,
                    "subscriptions/{sub}:pull",
                    json!({ "maxMessages": count, "returnImmediately": true }),
                )
                .await;
            response
                .get("receivedMessages")
                .map(|rm| rm.as_array().unwrap().len())
                .unwrap_or(0)
        }

        async fn request(&self, method: Method, base: &str, body: Value) -> Value {
            let path = base
                .replace("{topic}", &self.topic)
                .replace("{sub}", &self.subscription);
            let uri = [PROJECT_URI.as_str(), &path].join("/");
            let body = crate::serde::json::to_bytes(&body).unwrap().freeze();
            let request = Request::builder()
                .method(method)
                .uri(uri)
                .body(body.into())
                .unwrap();
            let response = self.client.send(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
            serde_json::from_str(&String::from_utf8(body.to_vec()).unwrap()).unwrap()
        }

        async fn shutdown_check(&self, shutdown: shutdown::SourceShutdownCoordinator) {
            self.shutdown(shutdown).await;
            components::SOURCE_TESTS.assert(&components::HTTP_PULL_SOURCE_TAGS);
        }

        async fn shutdown(&self, mut shutdown: shutdown::SourceShutdownCoordinator) {
            let deadline = Instant::now() + Duration::from_secs(1);
            let shutdown = shutdown.shutdown_source(&self.component, deadline);
            assert!(shutdown.await);
        }
    }

    async fn receive_events(rx: &mut (impl Stream<Item = Event> + Unpin), test_data: TestData) {
        let TestData {
            start,
            lines,
            attributes,
        } = test_data;

        let events: Vec<Event> = tokio::time::timeout(
            Duration::from_secs(1),
            test_util::collect_n_stream(rx, lines.len()),
        )
        .await
        .unwrap();

        let end = Utc::now();
        let mut message_ids = HashSet::new();

        assert_eq!(events.len(), lines.len());
        for (message, event) in lines.into_iter().zip(events) {
            let log = event.into_log();
            assert_eq!(log.get("message"), Some(&message.into()));
            assert_eq!(log.get("source_type"), Some(&"gcp_pubsub".into()));
            assert!(log.get("timestamp").unwrap().as_timestamp().unwrap() >= &start);
            assert!(log.get("timestamp").unwrap().as_timestamp().unwrap() <= &end);
            assert!(
                message_ids.insert(log.get("message_id").unwrap().clone().to_string()),
                "Message contained duplicate message_id"
            );
            let logattr = log
                .get("attributes")
                .expect("missing attributes")
                .as_object()
                .unwrap()
                .clone();
            assert_eq!(logattr.len(), attributes.len());
            for (a, b) in logattr.into_iter().zip(&attributes) {
                assert_eq!(&a.0, b.0);
                assert_eq!(a.1, b.1.clone().into());
            }
        }
    }
}
