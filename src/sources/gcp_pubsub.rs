use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    error::Error as _, future::Future, pin::Pin, sync::Arc, task::Context, task::Poll,
    time::Duration,
};

use chrono::NaiveDateTime;
use derivative::Derivative;
use futures::{stream, stream::FuturesUnordered, FutureExt, Stream, StreamExt, TryFutureExt};
use http::uri::{InvalidUri, Scheme, Uri};
use once_cell::sync::Lazy;
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{
    metadata::{errors::InvalidMetadataValue, MetadataValue},
    transport::{Certificate, ClientTlsConfig, Endpoint, Identity},
    Code, Request, Status,
};
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, EventsReceived, InternalEventHandle as _, Protocol, Registered,
};
use vector_lib::lookup::owned_value_path;
use vector_lib::{byte_size_of::ByteSizeOf, finalizer::UnorderedFinalizer};
use vrl::path;
use vrl::value::{kind::Collection, Kind};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{DataType, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput},
    event::{BatchNotifier, BatchStatus, Event, MaybeAsLogMut, Value},
    gcp::{GcpAuthConfig, GcpAuthenticator, Scope, PUBSUB_URL},
    internal_events::{
        GcpPubsubConnectError, GcpPubsubReceiveError, GcpPubsubStreamingPullError,
        StreamClosedError,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util,
    tls::{TlsConfig, TlsSettings},
    SourceSender,
};

const MIN_ACK_DEADLINE_SECS: u64 = 10;
const MAX_ACK_DEADLINE_SECS: u64 = 600;

// We use a bounded channel for the acknowledgement ID communication
// between the request stream and receiver. During benchmark runs,
// this channel had only a single element over 80% of the time and
// rarely went over 8 elements. Having it too small does not introduce
// deadlocks, as the worst case is slightly less efficient ack
// processing.
const ACK_QUEUE_SIZE: usize = 8;

type Finalizer = UnorderedFinalizer<Vec<String>>;

// prost emits some generated code that includes clones on `Arc`
// objects, which causes a clippy ding on this block. We don't
// directly control the generated code, so allow this lint here.
#[allow(clippy::clone_on_ref_ptr)]
// https://github.com/hyperium/tonic/issues/1350
#[allow(clippy::missing_const_for_fn)]
#[allow(warnings)]
mod proto {
    include!(concat!(env!("OUT_DIR"), "/google.pubsub.v1.rs"));

    use vector_lib::ByteSizeOf;

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
    Endpoint { source: tonic::transport::Error },
    #[snafu(display("Could not set up endpoint TLS settings: {}", source))]
    EndpointTls { source: tonic::transport::Error },
    #[snafu(display("Could not connect: {}", source))]
    Connect { source: tonic::transport::Error },
    #[snafu(display("Could not pull data from remote: {}", source))]
    Pull { source: Status },
    #[snafu(display(
        "`ack_deadline_secs` is outside the valid range of {} to {}",
        MIN_ACK_DEADLINE_SECS,
        MAX_ACK_DEADLINE_SECS
    ))]
    InvalidAckDeadline,
    #[snafu(display("Cannot set both `ack_deadline_secs` and `ack_deadline_seconds`"))]
    BothAckDeadlineSecsAndSeconds,
    #[snafu(display("Cannot set both `retry_delay_secs` and `retry_delay_seconds`"))]
    BothRetryDelaySecsAndSeconds,
}

static CLIENT_ID: Lazy<String> = Lazy::new(|| uuid::Uuid::new_v4().to_string());

/// Configuration for the `gcp_pubsub` source.
#[serde_as]
#[configurable_component(source(
    "gcp_pubsub",
    "Fetch observability events from GCP's Pub/Sub messaging system."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    /// The project name from which to pull logs.
    #[configurable(metadata(docs::examples = "my-log-source-project"))]
    pub project: String,

    /// The subscription within the project which is configured to receive logs.
    #[configurable(metadata(docs::examples = "my-vector-source-subscription"))]
    pub subscription: String,

    /// The endpoint from which to pull data.
    #[configurable(metadata(docs::examples = "https://us-central1-pubsub.googleapis.com"))]
    #[serde(default = "default_endpoint")]
    pub endpoint: String,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The maximum number of concurrent stream connections to open at once.
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,

    /// The number of messages in a response to mark a stream as
    /// "busy". This is used to determine if more streams should be
    /// started.
    ///
    /// The GCP Pub/Sub servers send responses with 100 or more messages when
    /// the subscription is busy.
    #[serde(default = "default_full_response")]
    pub full_response_size: usize,

    /// How often to poll the currently active streams to see if they
    /// are all busy and so open a new stream.
    #[serde(default = "default_poll_time")]
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Poll Time"))]
    pub poll_time_seconds: Duration,

    /// The acknowledgement deadline, in seconds, to use for this stream.
    ///
    /// Messages that are not acknowledged when this deadline expires may be retransmitted.
    #[serde(default = "default_ack_deadline")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Acknowledgement Deadline"))]
    pub ack_deadline_secs: Duration,

    /// The acknowledgement deadline, in seconds, to use for this stream.
    ///
    /// Messages that are not acknowledged when this deadline expires may be retransmitted.
    #[configurable(
        deprecated = "This option has been deprecated, use `ack_deadline_secs` instead."
    )]
    pub ack_deadline_seconds: Option<u16>,

    /// The amount of time, in seconds, to wait between retry attempts after an error.
    #[serde(default = "default_retry_delay")]
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Retry Delay"))]
    pub retry_delay_secs: Duration,

    /// The amount of time, in seconds, to wait between retry attempts after an error.
    #[configurable(
        deprecated = "This option has been deprecated, use `retry_delay_secs` instead."
    )]
    pub retry_delay_seconds: Option<f64>,

    /// The amount of time, in seconds, with no received activity
    /// before sending a keepalive request. If this is set larger than
    /// `60`, you may see periodic errors sent from the server.
    #[serde(default = "default_keepalive")]
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[configurable(metadata(docs::human_name = "Keepalive"))]
    pub keepalive_secs: Duration,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,
}

fn default_endpoint() -> String {
    PUBSUB_URL.to_string()
}

const fn default_ack_deadline() -> Duration {
    Duration::from_secs(600)
}

const fn default_retry_delay() -> Duration {
    Duration::from_secs(1)
}

const fn default_keepalive() -> Duration {
    Duration::from_secs(60)
}

const fn default_max_concurrency() -> usize {
    10
}

const fn default_full_response() -> usize {
    100
}

const fn default_poll_time() -> Duration {
    Duration::from_secs(2)
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_pubsub")]
impl SourceConfig for PubsubConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let ack_deadline_secs = match self.ack_deadline_seconds {
            None => self.ack_deadline_secs,
            Some(ads) => {
                warn!("The `ack_deadline_seconds` setting is deprecated, use `ack_deadline_secs` instead.");
                Duration::from_secs(ads as u64)
            }
        };
        if !(MIN_ACK_DEADLINE_SECS..=MAX_ACK_DEADLINE_SECS).contains(&ack_deadline_secs.as_secs()) {
            return Err(PubsubError::InvalidAckDeadline.into());
        }

        let retry_delay_secs = match self.retry_delay_seconds {
            None => self.retry_delay_secs,
            Some(rds) => {
                warn!("The `retry_delay_seconds` setting is deprecated, use `retry_delay_secs` instead.");
                Duration::from_secs_f64(rds)
            }
        };

        let auth = self.auth.build(Scope::PubSub).await?;

        let mut uri: Uri = self.endpoint.parse().context(UriSnafu)?;
        auth.apply_uri(&mut uri);

        let tls = TlsSettings::from_options(&self.tls)?;
        let host = uri.host().unwrap_or("pubsub.googleapis.com");
        let mut tls_config = ClientTlsConfig::new().domain_name(host);
        if let Some((cert, key)) = tls.identity_pem() {
            tls_config = tls_config.identity(Identity::from_pem(cert, key));
        }
        for authority in tls.authorities_pem() {
            tls_config = tls_config.ca_certificate(Certificate::from_pem(authority));
        }

        let mut endpoint: Endpoint = uri.to_string().parse().context(EndpointSnafu)?;
        if uri.scheme() != Some(&Scheme::HTTP) {
            endpoint = endpoint.tls_config(tls_config).context(EndpointTlsSnafu)?;
        }

        let token_generator = auth.spawn_regenerate_token();

        let protocol = uri
            .scheme()
            .map(|scheme| Protocol(scheme.to_string().into()))
            .unwrap_or(Protocol::HTTP);

        let source = PubsubSource {
            endpoint,
            auth,
            token_generator,
            subscription: format!(
                "projects/{}/subscriptions/{}",
                self.project, self.subscription
            ),
            decoder: DecodingConfig::new(
                self.framing.clone(),
                self.decoding.clone(),
                log_namespace,
            )
            .build()?,
            acknowledgements: cx.do_acknowledgements(self.acknowledgements),
            shutdown: cx.shutdown,
            out: cx.out,
            ack_deadline_secs,
            retry_delay: retry_delay_secs,
            keepalive: self.keepalive_secs,
            concurrency: Default::default(),
            full_response_size: self.full_response_size,
            log_namespace,
            bytes_received: register!(BytesReceived::from(protocol)),
            events_received: register!(EventsReceived),
        }
        .run_all(self.max_concurrency, self.poll_time_seconds)
        .map_err(|error| error!(message = "Source failed.", %error));
        Ok(Box::pin(source))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                PubsubConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_source_metadata(
                PubsubConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("attributes"))),
                &owned_value_path!("attributes"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                None,
            )
            .with_source_metadata(
                PubsubConfig::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("message_id"))),
                &owned_value_path!("message_id"),
                Kind::bytes(),
                None,
            );

        vec![SourceOutput::new_logs(DataType::Log, schema_definition)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl_generate_config_from_default!(PubsubConfig);

#[derive(Clone)]
struct PubsubSource {
    endpoint: Endpoint,
    auth: GcpAuthenticator,
    token_generator: watch::Receiver<()>,
    subscription: String,
    decoder: Decoder,
    acknowledgements: bool,
    ack_deadline_secs: Duration,
    shutdown: ShutdownSignal,
    out: SourceSender,
    retry_delay: Duration,
    keepalive: Duration,
    // The current concurrency is shared across all tasks. It is used
    // by the streams to avoid shutting down the last stream, which
    // would result in repeatedly re-opening the stream on idle.
    concurrency: Arc<AtomicUsize>,
    full_response_size: usize,
    log_namespace: LogNamespace,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
}

enum State {
    RetryNow,
    RetryDelay,
    Shutdown,
}

impl PubsubSource {
    async fn run_all(mut self, max_concurrency: usize, poll_time: Duration) -> crate::Result<()> {
        let mut tasks = FuturesUnordered::new();

        loop {
            self.concurrency.store(tasks.len(), Ordering::Relaxed);
            tokio::select! {
                _ = &mut self.shutdown => break,
                _ = tasks.next() => {
                    if tasks.is_empty() {
                        // Either no tasks were started or a race
                        // condition resulted in the last task
                        // exiting. Start up a new stream immediately.
                        self.start_one(&tasks);
                    }

                },
                _ = tokio::time::sleep(poll_time) => {
                    // If all of the tasks are marked as busy, start
                    // up a new one.
                    if tasks.len() < max_concurrency
                        && tasks.iter().all(|task| task.busy_flag.load(Ordering::Relaxed))
                    {
                        self.start_one(&tasks);
                    }
                }
            }
        }

        // Wait for all active streams to exit on shutdown
        while tasks.next().await.is_some() {}

        Ok(())
    }

    fn start_one(&self, tasks: &FuturesUnordered<Task>) {
        info!(message = "Starting stream.", concurrency = tasks.len() + 1);
        // The `busy_flag` is used to monitor the status of a
        // stream. It will start marked as idle to prevent the above
        // scan from spinning up too many at once. When a stream
        // receives "full" batches, it will mark itself as busy, and
        // when it has an idle interval it will mark itself as not
        // busy.
        let busy_flag = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(self.clone().run(Arc::clone(&busy_flag)));
        tasks.push(Task { task, busy_flag });
    }

    async fn run(mut self, busy_flag: Arc<AtomicBool>) {
        loop {
            match self.run_once(&busy_flag).await {
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
    }

    async fn run_once(&mut self, busy_flag: &Arc<AtomicBool>) -> State {
        let connection = match self.endpoint.connect().await {
            Ok(connection) => connection,
            Err(error) => {
                emit!(GcpPubsubConnectError { error });
                return State::RetryDelay;
            }
        };

        let mut client = proto::subscriber_client::SubscriberClient::with_interceptor(
            connection,
            |mut req: Request<()>| {
                if let Some(token) = self.auth.make_token() {
                    let authorization = MetadataValue::try_from(&token).map_err(|_| {
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

        let (ack_ids_sender, ack_ids_receiver) = mpsc::channel(ACK_QUEUE_SIZE);

        // Handle shutdown during startup, the streaming pull doesn't
        // start if there is no data in the subscription.
        let request_stream = self.request_stream(ack_ids_receiver);
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
            Finalizer::maybe_new(self.acknowledgements, Some(self.shutdown.clone()));
        let mut pending_acks = 0;

        loop {
            tokio::select! {
                biased;
                receipts = ack_stream.next() => if let Some((status, receipts)) = receipts {
                    pending_acks -= 1;
                    if status == BatchStatus::Delivered {
                        ack_ids_sender
                            .send(receipts)
                            .await
                            .unwrap_or_else(|_| unreachable!("request stream never closes"));
                    }
                },
                response = stream.next() => match response {
                    Some(Ok(response)) => {
                        self.handle_response(
                            response,
                            &finalizer,
                            &ack_ids_sender,
                            &mut pending_acks,
                            busy_flag,
                        ).await;
                    }
                    Some(Err(error)) => break translate_error(error),
                    None => break State::RetryNow,
                },
                _ = &mut self.shutdown, if pending_acks == 0 => return State::Shutdown,
                _ = self.token_generator.changed() => {
                    debug!("New authentication token generated, restarting stream.");
                    break State::RetryNow;
                },
                _ = tokio::time::sleep(self.keepalive) => {
                    if pending_acks == 0 {
                        // No pending acks, and no new data, so drop
                        // this stream if we aren't the only active
                        // one.
                        if self.concurrency.load(Ordering::Relaxed) > 1 {
                            info!("Shutting down inactive stream.");
                            break State::Shutdown;
                        }
                        // Otherwise, mark this stream as idle.
                        busy_flag.store(false, Ordering::Relaxed);
                    }
                    // GCP Pub/Sub likes to time out connections after
                    // about 75 seconds of inactivity. To forestall
                    // the resulting error, send an empty array of
                    // acknowledgement IDs to the request stream if no
                    // other activity has happened. This will result
                    // in a new request with empty fields, effectively
                    // a keepalive.
                    ack_ids_sender
                        .send(Vec::new())
                        .await
                        .unwrap_or_else(|_| unreachable!("request stream never closes"));
                }
            }
        }
    }

    fn request_stream(
        &self,
        ack_ids: mpsc::Receiver<Vec<String>>,
    ) -> impl Stream<Item = proto::StreamingPullRequest> + 'static {
        let subscription = self.subscription.clone();
        let client_id = CLIENT_ID.clone();
        let stream_ack_deadline_seconds = self.ack_deadline_secs.as_secs() as i32;
        let ack_ids = ReceiverStream::new(ack_ids).ready_chunks(ACK_QUEUE_SIZE);

        stream::once(async move {
            // These fields are only valid on the first request in the
            // stream, and so must not be repeated below.
            proto::StreamingPullRequest {
                subscription,
                client_id,
                stream_ack_deadline_seconds,
                ..Default::default()
            }
        })
        .chain(ack_ids.map(|chunks| {
            // These "requests" serve only to send updates about
            // acknowledgements to the server. None of the above
            // fields need to be repeated and, in fact, will cause
            // an stream error and cancellation if they are
            // present.
            proto::StreamingPullRequest {
                ack_ids: chunks.into_iter().flatten().collect(),
                ..Default::default()
            }
        }))
    }

    async fn handle_response(
        &mut self,
        response: proto::StreamingPullResponse,
        finalizer: &Option<Finalizer>,
        ack_ids: &mpsc::Sender<Vec<String>>,
        pending_acks: &mut usize,
        busy_flag: &Arc<AtomicBool>,
    ) {
        if response.received_messages.len() >= self.full_response_size {
            busy_flag.store(true, Ordering::Relaxed);
        }
        self.bytes_received.emit(ByteSize(response.size_of()));

        let (batch, notifier) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
        let (events, ids) = self.parse_messages(response.received_messages, batch).await;

        let count = events.len();
        match self.out.send_batch(events).await {
            Err(_) => emit!(StreamClosedError { count }),
            Ok(()) => match notifier {
                None => ack_ids
                    .send(ids)
                    .await
                    .unwrap_or_else(|_| unreachable!("request stream never closes")),
                Some(notifier) => {
                    finalizer
                        .as_ref()
                        .expect("Finalizer must have been set up for acknowledgements")
                        .add(ids, notifier);
                    *pending_acks += 1;
                }
            },
        }
    }

    async fn parse_messages(
        &self,
        response: Vec<proto::ReceivedMessage>,
        batch: Option<BatchNotifier>,
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
        &'a self,
        message: proto::PubsubMessage,
        batch: &'a Option<BatchNotifier>,
    ) -> impl Iterator<Item = Event> + 'a {
        let attributes = Value::Object(
            message
                .attributes
                .into_iter()
                .map(|(key, value)| (key.into(), Value::Bytes(value.into())))
                .collect(),
        );
        let log_namespace = self.log_namespace;
        util::decode_message(
            self.decoder.clone(),
            "gcp_pubsub",
            &message.data,
            message.publish_time.map(|dt| {
                NaiveDateTime::from_timestamp_opt(dt.seconds, dt.nanos as u32)
                    .expect("invalid timestamp")
                    .and_utc()
            }),
            batch,
            log_namespace,
            &self.events_received,
        )
        .map(move |mut event| {
            if let Some(log) = event.maybe_as_log_mut() {
                log_namespace.insert_source_metadata(
                    PubsubConfig::NAME,
                    log,
                    Some(LegacyKey::Overwrite(path!("message_id"))),
                    path!("message_id"),
                    message.message_id.clone(),
                );
                log_namespace.insert_source_metadata(
                    PubsubConfig::NAME,
                    log,
                    Some(LegacyKey::Overwrite(path!("attributes"))),
                    path!("attributes"),
                    attributes.clone(),
                )
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

#[pin_project::pin_project]
struct Task {
    task: tokio::task::JoinHandle<()>,
    busy_flag: Arc<AtomicBool>,
}

impl Future for Task {
    type Output = Result<(), tokio::task::JoinError>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        self.task.poll_unpin(ctx)
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::lookup::OwnedTargetPath;
    use vector_lib::schema::Definition;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PubsubConfig>();
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = PubsubConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("gcp_pubsub", "timestamp"),
                    Kind::timestamp().or_undefined(),
                    Some("timestamp"),
                )
                .with_metadata_field(
                    &owned_value_path!("gcp_pubsub", "attributes"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("gcp_pubsub", "message_id"),
                    Kind::bytes(),
                    None,
                );

        assert_eq!(definitions, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = PubsubConfig::default();

        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(
            &owned_value_path!("timestamp"),
            Kind::timestamp().or_undefined(),
            Some("timestamp"),
        )
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(
            &owned_value_path!("attributes"),
            Kind::object(Collection::empty().with_unknown(Kind::bytes())),
            None,
        )
        .with_event_field(&owned_value_path!("message_id"), Kind::bytes(), None);

        assert_eq!(definitions, Some(expected_definition));
    }
}

#[cfg(all(test, feature = "gcp-integration-tests"))]
mod integration_tests {
    use std::collections::{BTreeMap, HashSet};

    use base64::prelude::{Engine as _, BASE64_STANDARD};
    use chrono::{DateTime, Utc};
    use futures::{Stream, StreamExt};
    use http::method::Method;
    use hyper::{Request, StatusCode};
    use once_cell::sync::Lazy;
    use serde_json::{json, Value};
    use tokio::time::{Duration, Instant};
    use vrl::btreemap;

    use super::*;
    use crate::config::{ComponentKey, ProxyConfig};
    use crate::test_util::components::{assert_source_compliance, SOURCE_TAGS};
    use crate::test_util::{self, components, random_string};
    use crate::{event::EventStatus, gcp, http::HttpClient, shutdown, SourceSender};

    const PROJECT: &str = "sourceproject";
    static PROJECT_URI: Lazy<String> =
        Lazy::new(|| format!("{}/v1/projects/{}", *gcp::PUBSUB_ADDRESS, PROJECT));
    static ACK_DEADLINE: Lazy<Duration> = Lazy::new(|| Duration::from_secs(10)); // Minimum custom deadline allowed by Pub/Sub

    #[tokio::test]
    async fn oneshot() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;
            let test_data = tester.send_test_events(99, BTreeMap::new()).await;
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
        tester.send_test_events(1, BTreeMap::new()).await;
        assert!(rx.next().await.is_none());
        assert_eq!(tester.pull_count(1).await, 1);
    }

    #[tokio::test]
    async fn shuts_down_after_data_received() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let (tester, mut rx, shutdown) = setup(EventStatus::Delivered).await;

            let test_data = tester.send_test_events(1, BTreeMap::new()).await;
            receive_events(&mut rx, test_data).await;

            tester.shutdown_check(shutdown).await;

            assert!(rx.next().await.is_none());
            tester.send_test_events(1, BTreeMap::new()).await;
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
                let test_data = tester.send_test_events(9, BTreeMap::new()).await;
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

            let test_data = tester.send_test_events(1, BTreeMap::new()).await;
            receive_events(&mut rx, test_data).await;

            tester.shutdown_check(shutdown).await;

            // Make sure there are no messages left in the queue
            assert_eq!(tester.pull_count(10).await, 0);

            // Wait for the acknowledgement deadline to expire
            tokio::time::sleep(*ACK_DEADLINE + Duration::from_secs(1)).await;

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

            let test_data = tester.send_test_events(1, BTreeMap::new()).await;
            receive_events(&mut rx, test_data).await;

            tester.shutdown(shutdown).await;

            // Make sure there are no messages left in the queue
            assert_eq!(tester.pull_count(10).await, 0);

            // Wait for the acknowledgement deadline to expire
            tokio::time::sleep(*ACK_DEADLINE + Duration::from_secs(1)).await;

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
        NaiveDateTime::from_timestamp_opt(start, 0)
            .expect("invalid timestamp")
            .and_utc()
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
                "ackDeadlineSeconds": *ACK_DEADLINE,
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
                endpoint: gcp::PUBSUB_ADDRESS.clone(),
                auth: GcpAuthConfig {
                    skip_authentication: true,
                    ..Default::default()
                },
                ack_deadline_secs: *ACK_DEADLINE,
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
                .map(|input| BASE64_STANDARD.encode(input))
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
                assert_eq!(&a.0, b.0.as_str());
                assert_eq!(a.1, b.1.clone().into());
            }
        }
    }
}
