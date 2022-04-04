use std::sync::Arc;

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
    transport::{Certificate, Channel, ClientTlsConfig, Identity},
    Request,
};
use vector_core::ByteSizeOf;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{AcknowledgementsConfig, DataType, Output, SourceConfig, SourceContext},
    event::{BatchNotifier, BatchStatus, Event, MaybeAsLogMut, Value},
    gcp::{GcpAuthConfig, Scope, PUBSUB_URL},
    internal_events::{GcpPubsubReceiveError, HttpClientBytesReceived, StreamClosedError},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::util::{self},
    tls::{TlsOptions, TlsSettings},
};

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
enum PubsubError {
    #[snafu(display("Could not parse credentials metadata: {}", source))]
    Metadata { source: InvalidMetadataValue },
    #[snafu(display("Invalid endpoint URI: {}", source))]
    Uri { source: InvalidUri },
    #[snafu(display("Could not create channel: {}", source))]
    Channel { source: InvalidUri },
    #[snafu(display("Could not set up channel TLS settings: {}", source))]
    ChannelTls { source: tonic::transport::Error },
    #[snafu(display("Could not connect channel: {}", source))]
    Connect { source: tonic::transport::Error },
    #[snafu(display("Could not pull data from remote: {}", source))]
    Pull { source: tonic::Status },
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

    pub tls: Option<TlsOptions>,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: AcknowledgementsConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_pubsub")]
impl SourceConfig for PubsubConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let authorization = if self.skip_authentication {
            None
        } else {
            self.auth.make_credentials(Scope::PubSub).await?
        }
        .map(|credentials| MetadataValue::from_str(&credentials.make_token()))
        .transpose()
        .context(MetadataSnafu)?;

        let endpoint = self.endpoint.as_deref().unwrap_or(PUBSUB_URL).to_string();
        let uri: Uri = endpoint.parse().context(UriSnafu)?;
        let source = PubsubSource {
            endpoint,
            uri,
            authorization,
            subscription: format!(
                "projects/{}/subscriptions/{}",
                self.project, self.subscription
            ),
            decoder: DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build(),
            acknowledgements: cx.do_acknowledgements(&self.acknowledgements),
            tls: TlsSettings::from_options(&self.tls)?,
            cx,
            ack_ids: Default::default(),
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
    authorization: Option<MetadataValue<tonic::metadata::Ascii>>,
    subscription: String,
    decoder: Decoder,
    acknowledgements: bool,
    tls: TlsSettings,
    cx: SourceContext,
    // The acknowledgement IDs are pulled out of the response message
    // and then inserted into the request. However, the request is
    // generated in a separate async task from the response handling,
    // so the data needs to be shared this way.
    ack_ids: Arc<Mutex<Vec<String>>>,
}

impl PubsubSource {
    async fn run(mut self) -> crate::Result<()> {
        let mut channel = Channel::from_shared(self.endpoint.clone()).context(ChannelSnafu)?;
        if self.uri.scheme() != Some(&Scheme::HTTP) {
            channel = channel
                .tls_config(self.make_tls_config())
                .context(ChannelTlsSnafu)?;
        }
        let channel = channel.connect().await.context(ConnectSnafu)?;

        let mut stream = proto::subscriber_client::SubscriberClient::with_interceptor(
            channel,
            |mut req: Request<()>| {
                if let Some(authorization) = self.authorization.as_ref() {
                    req.metadata_mut()
                        .insert("authorization", authorization.clone());
                }
                Ok(req)
            },
        )
        .streaming_pull(self.request_stream())
        .await
        .context(PullSnafu)?
        .into_inner();

        while let Some(response) = stream.next().await {
            match response {
                Ok(response) => self.handle_response(response).await,
                Err(error) => emit!(GcpPubsubReceiveError { error }),
            }
        }

        Ok(())
    }

    fn make_tls_config(&self) -> ClientTlsConfig {
        let mut config = ClientTlsConfig::new().domain_name("pubsub.googleapis.com");
        if let Some((cert, key)) = self.tls.identity_pem() {
            config = config.identity(Identity::from_pem(cert, key));
        }
        for authority in self.tls.authorities_pem() {
            config = config.ca_certificate(Certificate::from_pem(authority));
        }
        config
    }

    fn request_stream(&self) -> impl Stream<Item = proto::StreamingPullRequest> + 'static {
        let mut subscription = Some(self.subscription.clone());
        let ack_ids = Arc::clone(&self.ack_ids);
        stream::repeat(()).then(move |()| {
            let ack_ids = Arc::clone(&ack_ids);
            let subscription = subscription.take().unwrap_or_else(String::new);
            async move {
                let mut ack_ids = ack_ids.lock().await;
                proto::StreamingPullRequest {
                    subscription,
                    ack_ids: std::mem::take(ack_ids.as_mut()),
                    stream_ack_deadline_seconds: 600,
                    client_id: CLIENT_ID.clone(),
                    max_outstanding_messages: 1024,
                    max_outstanding_bytes: 1024 * 1024,
                    ..Default::default()
                }
            }
        })
    }

    async fn handle_response(&mut self, response: proto::StreamingPullResponse) {
        emit!(HttpClientBytesReceived {
            byte_size: response.size_of(),
            protocol: self.uri.scheme().map(Scheme::as_str).unwrap_or("http"),
            endpoint: &self.endpoint,
        });

        let (batch, notifier) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
        let (events, ids) = self.parse_messages(response.received_messages, batch).await;

        let count = events.len();
        match self.cx.out.send_batch(events).await {
            Err(error) => emit!(StreamClosedError { error, count }),
            Ok(()) => match notifier {
                None => self.ack_ids.lock().await.extend(ids),
                Some(notifier) => {
                    let ack_ids = Arc::clone(&self.ack_ids);
                    tokio::spawn(async move {
                        if notifier.await == BatchStatus::Delivered {
                            ack_ids.lock().await.extend(ids);
                        }
                    });
                }
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
    use core::ops::Range;
    use std::collections::{BTreeMap, HashSet};

    use futures::Stream;
    use http::method::Method;
    use hyper::{Request, StatusCode};
    use once_cell::sync::Lazy;
    use serde_json::{json, Value};
    use vector_common::btreemap;

    use super::*;
    use crate::test_util::{collect_n_stream, components, random_string};
    use crate::{config::ProxyConfig, http::HttpClient, SourceSender};

    const PROJECT: &str = "sourceproject";
    static ADDRESS: Lazy<String> = Lazy::new(|| {
        std::env::var("EMULATOR_ADDRESS").unwrap_or_else(|_| "http://127.0.0.1:8681".into())
    });
    static PROJECT_URI: Lazy<String> =
        Lazy::new(|| format!("{}/v1/projects/{}", *ADDRESS, PROJECT));
    const ACK_DEADLINE: u64 = 1;

    #[tokio::test]
    async fn oneshot() {
        let (client, topic, _, mut rx) = setup().await;
        let start = now_trunc();
        let test_data = send_test_events(25..50, &client, &topic, btreemap![]).await;
        receive_events(&mut rx, start, test_data, btreemap![]).await;
    }

    #[tokio::test]
    async fn streams_data() {
        let (client, topic, _, mut rx) = setup().await;
        for _ in 0..10 {
            let start = now_trunc();
            let test_data = send_test_events(100..128, &client, &topic, btreemap![]).await;
            receive_events(&mut rx, start, test_data, btreemap![]).await;
        }
    }

    #[tokio::test]
    async fn sends_attributes() {
        let (client, topic, _, mut rx) = setup().await;
        let start = now_trunc();
        let attributes = btreemap![
            random_string(8) => random_string(88),
            random_string(8) => random_string(88),
            random_string(8) => random_string(88),
        ];
        let test_data = send_test_events(100..101, &client, &topic, attributes.clone()).await;
        receive_events(&mut rx, start, test_data, attributes).await;
    }

    #[tokio::test]
    async fn acks_received() {
        let (client, topic, subscription, mut rx) = setup().await;
        let start = now_trunc();

        let test_data = send_test_events(1..10, &client, &topic, btreemap![]).await;
        receive_events(&mut rx, start, test_data, btreemap![]).await;

        // Make sure there are no messages left in the queue
        assert_eq!(pull_count(&client, &subscription, 10).await, 0);

        // Wait for the acknowledgement deadline to expire
        tokio::time::sleep(std::time::Duration::from_secs(ACK_DEADLINE + 1)).await;

        // All messages are still acknowledged
        assert_eq!(pull_count(&client, &subscription, 10).await, 0);
    }

    async fn setup() -> (
        HttpClient,
        String,
        String,
        impl Stream<Item = Event> + Unpin,
    ) {
        components::init_test();

        let tls_settings = TlsSettings::from_options(&None).unwrap();
        let client = HttpClient::new(tls_settings, &ProxyConfig::default()).unwrap();
        let topic = make_topic(&client).await;
        let subscription = make_subscription(&client, &topic).await;

        let rx = make_source(subscription.clone()).await;

        (client, topic, subscription, rx)
    }

    fn now_trunc() -> DateTime<Utc> {
        let start = Utc::now().timestamp();
        // Truncate the milliseconds portion, the hard way.
        DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(start, 0), Utc)
    }

    async fn make_topic(client: &HttpClient) -> String {
        let topic = format!("topic-{}", random_string(10).to_lowercase());
        let uri = format!("{}/topics/{}", *PROJECT_URI, topic);
        request(client, Method::PUT, uri, json!({})).await;
        topic
    }

    async fn make_subscription(client: &HttpClient, topic: &str) -> String {
        let subscription = format!("sub-{}", random_string(10).to_lowercase());
        let uri = format!("{}/subscriptions/{}", *PROJECT_URI, subscription);
        let body = json!({
            "topic": format!("projects/{}/topics/{}", PROJECT, topic),
            "ackDeadlineSeconds": ACK_DEADLINE,
        });
        request(client, Method::PUT, uri, body).await;
        subscription
    }

    async fn make_source(subscription: String) -> impl Stream<Item = Event> + Unpin {
        let (tx, rx) = SourceSender::new_test();
        let config = PubsubConfig {
            project: PROJECT.into(),
            subscription,
            endpoint: Some(ADDRESS.clone()),
            skip_authentication: true,
            ..Default::default()
        };
        let source = config
            .build(SourceContext::new_test(tx, None))
            .await
            .expect("Failed to build source");
        tokio::spawn(async move { source.await.expect("Failed to run source") });

        rx
    }

    async fn send_test_events(
        range: Range<usize>,
        client: &HttpClient,
        topic: &str,
        attributes: BTreeMap<String, String>,
    ) -> Vec<String> {
        let test_data: Vec<_> = range.into_iter().map(random_string).collect();
        let messages: Vec<_> = test_data
            .iter()
            .map(|message| base64::encode(&message))
            .map(|data| json!({ "data": data, "attributes": attributes.clone() }))
            .collect();
        let uri = format!("{}/topics/{}:publish", *PROJECT_URI, topic);
        let body = json!({ "messages": messages });
        request(client, Method::POST, uri, body).await;
        test_data
    }

    async fn receive_events(
        rx: &mut (impl Stream<Item = Event> + Unpin),
        start: DateTime<Utc>,
        test_data: Vec<String>,
        attributes: BTreeMap<String, String>,
    ) {
        let events: Vec<Event> = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            collect_n_stream(rx, test_data.len()),
        )
        .await
        .unwrap();

        let end = Utc::now();
        let mut message_ids = HashSet::new();

        assert_eq!(events.len(), test_data.len());
        for (message, event) in test_data.into_iter().zip(events) {
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

        components::SOURCE_TESTS.assert(&components::HTTP_PULL_SOURCE_TAGS);
    }

    async fn pull_count(client: &HttpClient, subscription: &str, count: usize) -> usize {
        let response = request(
            &client,
            Method::POST,
            format!("{}/subscriptions/{}:pull", *PROJECT_URI, subscription),
            json!({ "maxMessages": count, "returnImmediately": true }),
        )
        .await;
        response
            .get("receivedMessages")
            .map(|rm| rm.as_array().unwrap().len())
            .unwrap_or(0)
    }

    async fn request(client: &HttpClient, method: Method, uri: String, body: Value) -> Value {
        let body = crate::serde::json::to_bytes(&body).unwrap().freeze();
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(body.into())
            .unwrap();
        let response = client.send(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        serde_json::from_str(&String::from_utf8(body.to_vec()).unwrap()).unwrap()
    }
}
