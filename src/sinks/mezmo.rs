use std::{io, time::SystemTime};

use bytes::Bytes;
use futures::{FutureExt, StreamExt, future, stream::BoxStream};
use http_1::{Request, StatusCode, Uri, uri::InvalidUri};
use serde_json::{
    json,
    value::{RawValue, to_raw_value},
};
use snafu::ResultExt;
use tower::{Service, ServiceBuilder};
use vector_http::empty_body;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    configurable::configurable_component,
    finalization::{EventFinalizers, EventStatus, Finalizable},
    internal_event::CountByteSize,
    json_size::JsonSize,
    partition::Partitioner,
    request_metadata::{GroupedCountByteSize, RequestMetadata},
    sensitive_string::SensitiveString,
    sink::StreamSink,
    stream::{
        BatcherSettings, DriverResponse,
        batcher::{config::BatchConfigParts, limiter::BatchLimiter},
    },
};
use vrl::{
    event_path,
    value::{Kind, Value},
};

use crate::{
    codecs::Transformer,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::Event,
    http::{Auth, client_v1::HttpClient},
    internal_events::{LargeEventDroppedError, SinkRequestBuildError},
    schema,
    sinks::{
        HTTPV1RequestBuilderSnafu,
        util::{
            BatchConfig, Compression, HttpEndpoint, RealtimeSizeBasedDefaultBatchSettings,
            RequestBuilder, ServiceBuilderExt, SinkBuilderExt, TowerRequestConfig,
            TowerRequestSettings,
            encoding::{Encoder as SinkEncoder, write_all},
            http::{HttpRequest, RetryStrategy},
            http_v1::{HttpService, HttpServiceRequestBuilder, http_response_retry_logic},
            metadata::RequestMetadataBuilder,
            request_builder::{EncodeResult, default_request_builder_concurrency_limit},
        },
    },
    template::{TemplateRenderingError, UnconfinedTemplate},
    tls::MaybeTlsSettings,
};

const PATH: &str = "/logs/ingest";

/// Configuration for the `mezmo` (formerly `logdna`) sink.
#[configurable_component(sink("mezmo", "Deliver log event data to Mezmo."))]
#[derive(Clone, Debug)]
pub struct MezmoConfig {
    /// The Ingestion API key.
    #[configurable(metadata(docs::examples = "${LOGDNA_API_KEY}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    api_key: SensitiveString,

    /// The HTTP endpoint to send logs to.
    ///
    /// Both IP address and hostname are accepted formats.
    #[serde(alias = "host")]
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "http://127.0.0.1"))]
    #[configurable(metadata(docs::examples = "http://example.com"))]
    endpoint: HttpEndpoint,

    /// The hostname that is attached to each batch of events.
    #[configurable(metadata(docs::examples = "${HOSTNAME}"))]
    #[configurable(metadata(docs::examples = "my-local-machine"))]
    hostname: UnconfinedTemplate,

    /// The MAC address that is attached to each batch of events.
    #[configurable(metadata(docs::examples = "my-mac-address"))]
    #[configurable(metadata(docs::human_name = "MAC Address"))]
    mac: Option<String>,

    /// The IP address that is attached to each batch of events.
    #[configurable(metadata(docs::examples = "0.0.0.0"))]
    #[configurable(metadata(docs::human_name = "IP Address"))]
    ip: Option<String>,

    /// The tags that are attached to each batch of events.
    #[configurable(metadata(docs::examples = "tag1"))]
    #[configurable(metadata(docs::examples = "tag2"))]
    tags: Option<Vec<UnconfinedTemplate>>,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// The default app that is set for events that do not contain a `file` or `app` field.
    #[serde(default = "default_app")]
    #[configurable(metadata(docs::examples = "my-app"))]
    default_app: String,

    /// The default environment that is set for events that do not contain an `env` field.
    #[serde(default = "default_env")]
    #[configurable(metadata(docs::examples = "staging"))]
    default_env: String,

    #[serde(default)]
    batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse("https://logs.mezmo.com").unwrap()
}

fn default_app() -> String {
    "vector".to_owned()
}

fn default_env() -> String {
    "production".to_owned()
}

impl GenerateConfig for MezmoConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r"hostname: hostname
            api_key: ${LOGDNA_API_KEY}",
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mezmo")]
impl SinkConfig for MezmoConfig {
    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty()
            .optional_meaning("timestamp", Kind::timestamp())
            .optional_meaning("message", Kind::bytes());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedMezmo {
    request_settings: TowerRequestSettings,
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for MezmoConfig {
    type Validated = ValidatedMezmo;

    fn validate(&self) -> crate::Result<ValidatedMezmo> {
        let request_settings = self.request.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;
        Ok(ValidatedMezmo {
            request_settings,
            batch_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedMezmo,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let ValidatedMezmo {
            request_settings,
            batch_settings,
        } = validated;

        let client = HttpClient::new(MaybeTlsSettings::from_config(None, false)?, cx.proxy())?;

        let service_request_builder = MezmoServiceRequestBuilder {
            endpoint: self.endpoint.clone(),
            mac: self.mac.clone(),
            ip: self.ip.clone(),
            auth: Auth::Basic {
                user: self.api_key.inner().to_string(),
                password: SensitiveString::default(),
            },
        };
        let service =
            HttpService::new(client.clone(), service_request_builder).with_response_decompression();
        let service = ServiceBuilder::new()
            .settings(
                request_settings.clone(),
                http_response_retry_logic(RetryStrategy::Default),
            )
            .service(service);

        let sink = MezmoSink {
            service,
            batch_settings: *batch_settings,
            encoder: self.build_encoder(batch_settings.size_limit),
        };

        let healthcheck = healthcheck(self.endpoint.clone(), client).boxed();

        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl MezmoConfig {
    fn build_encoder(&self, max_bytes: usize) -> MezmoEventEncoder {
        MezmoEventEncoder {
            hostname: self.hostname.clone(),
            tags: self.tags.clone(),
            transformer: self.encoding.clone(),
            default_app: self.default_app.clone(),
            default_env: self.default_env.clone(),
            max_bytes,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PartitionKey {
    hostname: String,
    tags: Option<Vec<String>>,
}

/// One event, encoded before batching so that batches are sized by the serialized line.
struct MezmoLine {
    /// `None` for an event larger than `batch.max_bytes`. Its finalizers and original size
    /// still travel with a batch, as they did in the legacy JSON array batch.
    line: Option<Box<RawValue>>,
    key: PartitionKey,
    finalizers: EventFinalizers,
    byte_size: usize,
    json_size: JsonSize,
}

impl Finalizable for MezmoLine {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl ByteSizeOf for MezmoLine {
    fn allocated_bytes(&self) -> usize {
        self.line.as_ref().map_or(0, |line| line.get().len())
            + self.key.hostname.allocated_bytes()
            + self.key.tags.allocated_bytes()
    }
}

struct MezmoEventEncoder {
    hostname: UnconfinedTemplate,
    tags: Option<Vec<UnconfinedTemplate>>,
    transformer: Transformer,
    default_app: String,
    default_env: String,
    max_bytes: usize,
}

impl MezmoEventEncoder {
    fn render_key(
        &self,
        event: &Event,
    ) -> Result<PartitionKey, (Option<&str>, TemplateRenderingError)> {
        let hostname = self
            .hostname
            .render_string(event)
            .map_err(|e| (Some("hostname"), e))?;
        let tags = self.tags.as_ref().map_or(Ok(None), |tags| {
            let mut vec = Vec::with_capacity(tags.len());
            for tag in tags {
                vec.push(tag.render_string(event).map_err(|e| (None, e))?);
            }
            Ok(Some(vec))
        })?;
        Ok(PartitionKey { hostname, tags })
    }

    /// Encodes `event` into its line, or drops it when its partition key cannot be rendered.
    fn encode_event(&self, mut event: Event) -> Option<MezmoLine> {
        let key = self
            .render_key(&event)
            .map_err(|(field, error)| {
                emit!(crate::internal_events::TemplateRenderingError {
                    error,
                    field,
                    drop_event: true,
                });
            })
            .ok()?;

        let finalizers = event.metadata_mut().take_finalizers();
        let byte_size = event.size_of();
        let json_size = event.estimated_json_encoded_size_of();

        self.transformer.transform(&mut event);
        let mut log = event.into_log();

        let line = log
            .message_path()
            .cloned()
            .as_ref()
            .and_then(|path| log.remove(path))
            .unwrap_or_else(|| String::new().into());

        let timestamp: Value = log
            .timestamp_path()
            .cloned()
            .and_then(|path| log.remove(&path))
            .unwrap_or_else(|| chrono::Utc::now().into());

        let mut map = serde_json::map::Map::new();

        map.insert("line".to_string(), json!(line));
        map.insert("timestamp".to_string(), json!(timestamp));

        if let Some(env) = log.remove(event_path!("env")) {
            map.insert("env".to_string(), json!(env));
        }

        if let Some(app) = log.remove(event_path!("app")) {
            map.insert("app".to_string(), json!(app));
        }

        if let Some(file) = log.remove(event_path!("file")) {
            map.insert("file".to_string(), json!(file));
        }

        if !map.contains_key("env") {
            map.insert("env".to_string(), json!(self.default_env));
        }

        if !map.contains_key("app") && !map.contains_key("file") {
            map.insert("app".to_string(), json!(self.default_app.as_str()));
        }

        if !log.is_empty_object() {
            map.insert("meta".into(), json!(&log));
        }

        let line = to_raw_value(&map).expect("Value should be valid json");
        // Each line costs its length plus one delimiter byte. A line that cannot fit an empty
        // batch is dropped, but its finalizers and sizes stay in a batch.
        let length = line.get().len();
        let line = if length + 1 >= self.max_bytes {
            emit!(LargeEventDroppedError {
                length,
                max_length: self.max_bytes,
            });
            None
        } else {
            Some(line)
        };

        Some(MezmoLine {
            line,
            key,
            finalizers,
            byte_size,
            json_size,
        })
    }
}

struct MezmoPartitioner;

impl Partitioner for MezmoPartitioner {
    type Item = MezmoLine;
    type Key = PartitionKey;

    fn partition(&self, item: &MezmoLine) -> PartitionKey {
        item.key.clone()
    }
}

/// Limits a batch by its encoded lines. Dropped oversized events count toward neither limit.
struct MezmoBatchLimiter {
    max_bytes: usize,
    max_events: usize,
    bytes: usize,
    lines: usize,
}

impl BatchLimiter<MezmoLine, Vec<MezmoLine>> for MezmoBatchLimiter {
    /// The line cost, or `None` for a dropped oversized event.
    type ItemMetadata = Option<usize>;

    fn is_batch_full(&self, _batch: &Vec<MezmoLine>) -> bool {
        self.lines >= self.max_events || self.bytes >= self.max_bytes
    }

    fn item_fits_in_batch(
        &self,
        item: &MezmoLine,
        _batch: &Vec<MezmoLine>,
    ) -> (bool, Option<usize>) {
        let cost = item.line.as_ref().map(|line| line.get().len() + 1);
        // A dropped event joins a batch without lines; otherwise it starts the next batch.
        let fits = self.lines == 0 || cost.is_some_and(|cost| self.bytes + cost <= self.max_bytes);
        (fits, cost)
    }

    fn push_item(&mut self, cost: Option<usize>) {
        if let Some(cost) = cost {
            self.lines += 1;
            self.bytes += cost;
        }
    }

    fn reset(&mut self) {
        self.lines = 0;
        self.bytes = 0;
    }
}

const fn batch_config(
    settings: BatcherSettings,
) -> BatchConfigParts<MezmoBatchLimiter, Vec<MezmoLine>> {
    BatchConfigParts {
        batch_limiter: MezmoBatchLimiter {
            max_bytes: settings.size_limit,
            max_events: settings.item_limit,
            bytes: 0,
            lines: 0,
        },
        batch_data: Vec::new(),
        timeout: settings.timeout,
    }
}

struct MezmoBodyEncoder;

impl SinkEncoder<Vec<MezmoLine>> for MezmoBodyEncoder {
    fn encode_input(
        &self,
        lines: Vec<MezmoLine>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let count = lines.len();
        let mut json_size = JsonSize::zero();
        let prefix = b"{\"lines\":[";
        let suffix = b"]}";
        let mut line_bytes = 0;
        write_all(writer, count, prefix)?;
        for line in lines {
            json_size += line.json_size;
            if let Some(line) = line.line {
                if line_bytes != 0 {
                    write_all(writer, count, b",")?;
                    line_bytes += 1;
                }
                let bytes = line.get().as_bytes();
                write_all(writer, count, bytes)?;
                line_bytes += bytes.len();
            }
        }
        write_all(writer, count, suffix)?;
        // Sent events are counted untagged from the original events, including dropped ones.
        Ok((
            prefix.len() + line_bytes + suffix.len(),
            CountByteSize(count, json_size).into(),
        ))
    }
}

struct MezmoRequestBuilder {
    encoder: MezmoBodyEncoder,
}

impl RequestBuilder<(PartitionKey, Vec<MezmoLine>)> for MezmoRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<MezmoLine>;
    type Encoder = MezmoBodyEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        (key, mut lines): (PartitionKey, Vec<MezmoLine>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = lines.take_finalizers();
        let events_byte_size = lines.iter().map(|line| line.byte_size).sum();
        // `RequestMetadataBuilder::build` takes the grouped JSON size from the encoder.
        let builder = RequestMetadataBuilder::new(
            lines.len(),
            events_byte_size,
            GroupedCountByteSize::new_untagged(),
        );
        ((key, finalizers), builder, lines)
    }

    fn build_request(
        &self,
        (key, finalizers): Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), finalizers, request_metadata, key)
    }
}

#[derive(Clone)]
struct MezmoServiceRequestBuilder {
    endpoint: HttpEndpoint,
    mac: Option<String>,
    ip: Option<String>,
    auth: Auth,
}

impl HttpServiceRequestBuilder<PartitionKey> for MezmoServiceRequestBuilder {
    fn build(
        &self,
        mut request: HttpRequest<PartitionKey>,
    ) -> Result<Request<Bytes>, crate::Error> {
        let key = request.get_additional_metadata();
        let mut query = url::form_urlencoded::Serializer::new(String::new());

        // Built per attempt, so a retried request reports the time it is sent.
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time can't drift behind the epoch!")
            .as_millis();

        query.append_pair("hostname", &key.hostname);
        query.append_pair("now", &now.to_string());

        if let Some(mac) = &self.mac {
            query.append_pair("mac", mac);
        }

        if let Some(ip) = &self.ip {
            query.append_pair("ip", ip);
        }

        if let Some(tags) = &key.tags {
            let tags = tags.join(",");
            query.append_pair("tags", &tags);
        }

        let query = query.finish();
        let uri = build_uri(&self.endpoint, &query)?;

        let mut http_request = Request::post(uri)
            .header("Content-Type", "application/json")
            .body(request.take_payload())
            .context(HTTPV1RequestBuilderSnafu)?;

        self.auth.apply_v1(&mut http_request);

        Ok(http_request)
    }
}

struct MezmoSink<S> {
    service: S,
    batch_settings: BatcherSettings,
    encoder: MezmoEventEncoder,
}

impl<S> MezmoSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let Self {
            service,
            batch_settings,
            encoder,
        } = *self;

        input
            .filter_map(move |event| future::ready(encoder.encode_event(event)))
            .batched_partitioned(MezmoPartitioner, batch_settings.timeout, move |_| {
                batch_config(batch_settings)
            })
            .request_builder(
                default_request_builder_concurrency_limit(),
                MezmoRequestBuilder {
                    encoder: MezmoBodyEncoder,
                },
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(request) => Some(request),
                }
            })
            .into_driver(service)
            .with_request_error_status(EventStatus::Errored)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for MezmoSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

fn build_uri(endpoint: &HttpEndpoint, query: &str) -> Result<Uri, InvalidUri> {
    format!("{}{PATH}?{query}", endpoint.as_uri()).parse()
}

async fn healthcheck(endpoint: HttpEndpoint, client: HttpClient) -> crate::Result<()> {
    let uri = build_uri(&endpoint, "")?;

    let request = Request::post(uri).body(empty_body())?;

    let response = client.send(request).await?;

    if response.status().is_server_error() {
        return Err("Server returned a server error".into());
    }

    if response.status() == StatusCode::FORBIDDEN {
        return Err("Token is not valid, 403 returned.".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, time::Duration};

    use futures::{StreamExt, channel::mpsc};
    use futures_util::stream;
    use http_0::{StatusCode, request::Parts};
    use serde_json::json;
    use vector_lib::event::{BatchNotifier, BatchStatus, Event, EventStatus, LogEvent};

    use super::*;
    use crate::{
        config::SinkConfig,
        sinks::util::test::{build_test_server_status, load_sink},
        test_util::{
            addr::next_addr,
            components::{HTTP_SINK_TAGS, assert_sink_compliance},
            random_lines,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MezmoConfig>();
    }

    #[test]
    fn encode_event() {
        let (config, _cx) = load_sink::<MezmoConfig>(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
            default_env = "acceptance"
            codec.except_fields = ["magic"]
        "#,
        )
        .unwrap();
        let encoder = config.build_encoder(usize::MAX);
        let encode = |event: Event| -> serde_json::Value {
            let line = encoder.encode_event(event).unwrap().line.unwrap();
            serde_json::from_str(line.get()).unwrap()
        };

        let mut event1 = Event::Log(LogEvent::from("hello world"));
        event1.as_mut_log().insert(event_path!("app"), "notvector");
        event1.as_mut_log().insert(event_path!("magic"), "vector");

        let mut event2 = Event::Log(LogEvent::from("hello world"));
        event2.as_mut_log().insert(event_path!("file"), "log.txt");

        let event3 = Event::Log(LogEvent::from("hello world"));

        let mut event4 = Event::Log(LogEvent::from("hello world"));
        event4.as_mut_log().insert(event_path!("env"), "staging");

        let event1_out = encode(event1);
        let event1_out = event1_out.as_object().unwrap();
        let event2_out = encode(event2);
        let event2_out = event2_out.as_object().unwrap();
        let event3_out = encode(event3);
        let event3_out = event3_out.as_object().unwrap();
        let event4_out = encode(event4);
        let event4_out = event4_out.as_object().unwrap();

        assert_eq!(event1_out.get("app").unwrap(), &json!("notvector"));
        assert_eq!(event2_out.get("file").unwrap(), &json!("log.txt"));
        assert_eq!(event3_out.get("app").unwrap(), &json!("vector"));
        assert_eq!(event3_out.get("env").unwrap(), &json!("acceptance"));
        assert_eq!(event4_out.get("env").unwrap(), &json!("staging"));
    }

    #[test]
    fn rejects_non_http_endpoint() {
        let config = serde_yaml::from_str::<MezmoConfig>(
            r#"
            api_key: "mylogtoken"
            hostname: "vector"
            endpoint: "ftp://logs.example.com"
        "#,
        );
        assert!(config.is_err());
    }

    async fn smoke_start(
        status_code: StatusCode,
        batch_status: BatchStatus,
    ) -> (
        Vec<&'static str>,
        Vec<Vec<String>>,
        mpsc::Receiver<(Parts, bytes::Bytes)>,
    ) {
        let (mut config, cx) = load_sink::<MezmoConfig>(
            r#"
            api_key = "mylogtoken"
            ip = "127.0.0.1"
            mac = "some-mac-addr"
            hostname = "{{ hostname }}"
            tags = ["test","maybeanothertest"]
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        _ = SinkConfig::build(&config, cx.clone()).await.unwrap();

        let (_guard, addr) = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        config.endpoint = HttpEndpoint::parse(&format!("http://{addr}")).unwrap();

        let (sink, _) = SinkConfig::build(&config, cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, status_code);
        tokio::spawn(server);

        let lines = random_lines(100).take(10).collect::<Vec<_>>();
        let mut events = Vec::new();
        let hosts = vec!["host0", "host1"];

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let mut partitions = vec![Vec::new(), Vec::new()];
        // Create 10 events where the first one contains custom
        // fields that are not just `message`.
        for (i, line) in lines.iter().enumerate() {
            let mut event = LogEvent::from(line.as_str()).with_batch_notifier(&batch);
            let p = i % 2;
            event.insert(event_path!("hostname"), hosts[p]);

            partitions[p].push(line.into());
            events.push(Event::Log(event));
        }
        drop(batch);

        let events = stream::iter(events).map(Into::into);
        sink.run(events).await.expect("Running sink failed");

        assert_eq!(receiver.try_recv(), Ok(batch_status));

        (hosts, partitions, rx)
    }

    #[tokio::test]
    async fn smoke_fails() {
        let (_hosts, _partitions, mut rx) =
            smoke_start(StatusCode::FORBIDDEN, BatchStatus::Rejected).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn smoke() {
        assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let (hosts, partitions, mut rx) =
                smoke_start(StatusCode::OK, BatchStatus::Delivered).await;

            for _ in 0..partitions.len() {
                let output = rx.next().await.unwrap();

                let request = &output.0;
                let body: serde_json::Value = serde_json::from_slice(&output.1[..]).unwrap();

                let query = request.uri.query().unwrap();

                let (p, host) = hosts
                    .iter()
                    .enumerate()
                    .find(|(_, host)| query.contains(&format!("hostname={host}")))
                    .expect("invalid hostname");
                let lines = &partitions[p];

                assert_eq!(request.method, http_0::Method::POST);
                assert_eq!(request.headers["content-type"], "application/json");
                assert_eq!(request.headers["authorization"], "Basic bXlsb2d0b2tlbjo=");
                assert!(query.contains("now="));
                assert!(query.contains("ip=127.0.0.1"));
                assert!(query.contains("mac=some-mac-addr"));
                assert!(query.contains("tags=test%2Cmaybeanothertest"));

                let output = body
                    .as_object()
                    .unwrap()
                    .get("lines")
                    .unwrap()
                    .as_array()
                    .unwrap();

                for (i, line) in output.iter().enumerate() {
                    // All lines are json objects
                    let line = line.as_object().unwrap();

                    assert_eq!(line.get("app").unwrap(), &json!("vector"));
                    assert_eq!(line.get("env").unwrap(), &json!("production"));
                    assert_eq!(line.get("line").unwrap(), &json!(lines[i]));

                    assert_eq!(
                        line.get("meta").unwrap(),
                        &json!({
                            "hostname": host,
                        })
                    );
                }
            }
        })
        .await;
    }

    fn long_message() -> String {
        "oversized ".repeat(20)
    }

    fn fixed_event(message: &str) -> LogEvent {
        let mut log = LogEvent::from(message);
        log.insert(
            event_path!("timestamp"),
            chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        log
    }

    fn encoder_with_max_bytes(max_bytes: usize) -> MezmoEventEncoder {
        let (config, _cx) = load_sink::<MezmoConfig>(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
        "#,
        )
        .unwrap();
        config.build_encoder(max_bytes)
    }

    /// The batch cost of `message`: its serialized line plus one delimiter byte.
    fn line_cost(message: &str) -> usize {
        let line = encoder_with_max_bytes(usize::MAX)
            .encode_event(fixed_event(message).into())
            .unwrap()
            .line
            .unwrap();
        line.get().len() + 1
    }

    /// Batches `messages` as the sink does. `None` is a dropped oversized event.
    async fn batch_messages(
        max_bytes: usize,
        max_events: usize,
        messages: &[&str],
    ) -> Vec<Vec<Option<String>>> {
        let encoder = encoder_with_max_bytes(max_bytes);
        let settings = BatcherSettings::new(
            Duration::from_mins(1),
            NonZeroUsize::new(max_bytes).unwrap(),
            NonZeroUsize::new(max_events).unwrap(),
        );
        let lines = messages
            .iter()
            .filter_map(|message| encoder.encode_event(fixed_event(message).into()))
            .collect::<Vec<_>>();

        stream::iter(lines)
            .batched_partitioned(MezmoPartitioner, settings.timeout, |_| {
                batch_config(settings)
            })
            .map(|(_, batch)| {
                batch
                    .into_iter()
                    .map(|line| {
                        line.line.map(|line| {
                            let line: serde_json::Value = serde_json::from_str(line.get()).unwrap();
                            line["line"].as_str().unwrap().to_owned()
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
            .await
    }

    #[tokio::test]
    async fn batches_by_serialized_line_size() {
        let cost = line_cost("a");

        // The second line fills the batch exactly, which sends it.
        assert_eq!(
            batch_messages(2 * cost, 100, &["a", "b", "c"]).await,
            vec![
                vec![Some("a".to_owned()), Some("b".to_owned())],
                vec![Some("c".to_owned())]
            ]
        );
        // A line that costs `max_bytes` is dropped; one more byte fits it.
        assert_eq!(batch_messages(cost, 100, &["a"]).await, vec![vec![None]]);
        assert_eq!(
            batch_messages(cost + 1, 100, &["a"]).await,
            vec![vec![Some("a".to_owned())]]
        );
        assert_eq!(
            batch_messages(usize::MAX, 2, &["a", "b", "c"]).await,
            vec![
                vec![Some("a".to_owned()), Some("b".to_owned())],
                vec![Some("c".to_owned())]
            ]
        );
    }

    #[tokio::test]
    async fn oversized_event_moves_to_next_batch() {
        let long = long_message();
        let max_bytes = line_cost(&long);

        // An oversized event sends the open batch and joins the next one.
        assert_eq!(
            batch_messages(max_bytes, 100, &["a", &long, "b"]).await,
            vec![vec![Some("a".to_owned())], vec![None, Some("b".to_owned())]]
        );
        // At the end of the input, it is sent alone.
        assert_eq!(
            batch_messages(max_bytes, 100, &["a", &long]).await,
            vec![vec![Some("a".to_owned())], vec![None]]
        );
        // It does not count toward the event limit.
        assert_eq!(
            batch_messages(max_bytes, 2, &[&long, "a", "b", "c"]).await,
            vec![
                vec![None, Some("a".to_owned()), Some("b".to_owned())],
                vec![Some("c".to_owned())]
            ]
        );
    }

    #[test]
    fn oversized_event_is_finalized_and_counted_with_its_batch() {
        let long = long_message();
        let encoder = encoder_with_max_bytes(line_cost(&long));
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let oversized = Event::from(fixed_event(&long).with_batch_notifier(&batch));
        let small = Event::from(fixed_event("a"));
        drop(batch);
        let json_size =
            oversized.estimated_json_encoded_size_of() + small.estimated_json_encoded_size_of();

        let lines = vec![
            encoder.encode_event(oversized).unwrap(),
            encoder.encode_event(small).unwrap(),
        ];
        let small_line = lines[1].line.as_ref().unwrap().get().to_owned();
        let key = lines[0].key.clone();
        let request_builder = MezmoRequestBuilder {
            encoder: MezmoBodyEncoder,
        };
        let ((_, finalizers), metadata_builder, events) = request_builder.split_input((key, lines));
        let payload = request_builder.encode_events(events).unwrap();
        let metadata = metadata_builder.build(&payload);

        assert_eq!(
            payload.payload,
            Bytes::from(format!(r#"{{"lines":[{small_line}]}}"#))
        );
        assert_eq!(metadata.event_count(), 2);
        assert_eq!(
            metadata.events_estimated_json_encoded_byte_size().size(),
            Some(CountByteSize(2, json_size))
        );

        finalizers.update_status(EventStatus::Rejected);
        drop(finalizers);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    /// Sends a small event and then an oversized one, returning the status of each.
    async fn run_with_oversized_tail(
        status_code: StatusCode,
    ) -> (
        BatchStatus,
        BatchStatus,
        mpsc::Receiver<(Parts, bytes::Bytes)>,
    ) {
        let long = long_message();
        let (mut config, cx) = load_sink::<MezmoConfig>(&format!(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
            batch.max_bytes = {}
        "#,
            line_cost(&long)
        ))
        .unwrap();

        let (_guard, addr) = next_addr();
        config.endpoint = HttpEndpoint::parse(&format!("http://{addr}")).unwrap();
        let (sink, _) = SinkConfig::build(&config, cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, status_code);
        tokio::spawn(server);

        let (small_batch, mut small_receiver) = BatchNotifier::new_with_receiver();
        let (oversized_batch, mut oversized_receiver) = BatchNotifier::new_with_receiver();
        let events = [
            Event::from(fixed_event("a").with_batch_notifier(&small_batch)),
            Event::from(fixed_event(&long).with_batch_notifier(&oversized_batch)),
        ];
        drop(small_batch);
        drop(oversized_batch);

        sink.run(stream::iter(events).map(Into::into))
            .await
            .expect("Running sink failed");

        (
            small_receiver.try_recv().unwrap(),
            oversized_receiver.try_recv().unwrap(),
            rx,
        )
    }

    #[tokio::test]
    async fn oversized_tail_event_is_sent_with_empty_lines() {
        let (small, oversized, mut rx) = run_with_oversized_tail(StatusCode::OK).await;
        assert_eq!(small, BatchStatus::Delivered);
        assert_eq!(oversized, BatchStatus::Delivered);

        let mut bodies = [rx.next().await.unwrap().1, rx.next().await.unwrap().1];
        bodies.sort();
        assert_eq!(bodies[0], bytes::Bytes::from_static(br#"{"lines":[]}"#));
        let body: serde_json::Value = serde_json::from_slice(&bodies[1]).unwrap();
        let lines = body["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["line"], json!("a"));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn oversized_event_takes_status_of_its_request() {
        let (small, oversized, _rx) = run_with_oversized_tail(StatusCode::FORBIDDEN).await;
        assert_eq!(small, BatchStatus::Rejected);
        assert_eq!(oversized, BatchStatus::Rejected);
    }

    #[tokio::test]
    async fn transport_error_after_retries_is_errored() {
        let (mut config, cx) = load_sink::<MezmoConfig>(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
            request.retry_attempts = 0
        "#,
        )
        .unwrap();

        // Nothing listens on the reserved port, so the connection is refused.
        let (_guard, addr) = next_addr();
        config.endpoint = HttpEndpoint::parse(&format!("http://{addr}")).unwrap();
        let (sink, _) = SinkConfig::build(&config, cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = Event::from(fixed_event("a").with_batch_notifier(&batch));
        drop(batch);

        sink.run(stream::iter([event]).map(Into::into))
            .await
            .expect("Running sink failed");

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));
    }
}
