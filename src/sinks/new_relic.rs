use crate::{
    buffers::Acker,
    config::{
        DataType, SinkConfig, SinkContext, SinkDescription
    },
    event::{
        Event, Value, MetricValue
    },
    http::HttpClient,
    sinks::util::{
        service::ServiceBuilderExt,
        builder::SinkBuilderExt,
        retries::RetryLogic,
        batch::{
            BatchError, BatchSize
        },
        encoding::{
            Encoder, EncodingConfigFixed
        },
        Batch, PushResult, BatchConfig, BatchSettings, Compression, TowerRequestConfig, StreamSink, RequestBuilder
    },
    tls::TlsSettings,
};
use hyper::Body;
use tracing::Instrument;
use vector_core::{
    stream::BatcherSettings,
    partition::NullPartitioner,
    event::{
        EventStatus, Finalizable, EventFinalizers
    },
    buffers::Ackable
};
use async_trait::async_trait;
use futures::{
    stream::{
        BoxStream, StreamExt
    },
    future::{
        self, BoxFuture
    },
    FutureExt
};
use http::{
    Request, Uri,
    header::{
        CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE
    }
};
use serde::{
    Deserialize, Serialize
};
use chrono::{
    DateTime, Utc
};
use std::{
    fmt::Debug,
    collections::HashMap,
    convert::TryFrom,
    io,
    time::SystemTime,
    task::{
        Context, Poll
    },
    num::NonZeroUsize,
    sync::Arc
};
use tower::{
    Service, ServiceBuilder
};

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

impl Encoder<Result<NewRelicApiModel, &'static str>> for EncodingConfigFixed<Encoding> {
    fn encode_input(&self, input: Result<NewRelicApiModel, &'static str>, writer: &mut dyn io::Write) -> io::Result<usize> {
        if let Ok(api_model) = input {
            let json = match api_model {
                NewRelicApiModel::Events(ev_api_model) => to_json(&ev_api_model),
                NewRelicApiModel::Metrics(met_api_model) => to_json(&met_api_model),
                NewRelicApiModel::Logs(log_api_model) => to_json(&log_api_model),
            };
            if let Some(json) = json {
                let size = writer.write(&json)?;
                io::Result::Ok(size)
            }
            else {
                io::Result::Ok(0)
            }
        }
        else {
            io::Result::Ok(0)
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Copy, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicRegion {
    #[derivative(Default)]
    Us,
    Eu,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Copy, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicApi {
    #[derivative(Default)]
    Events,
    Metrics,
    Logs
}

pub fn to_json<T: Serialize>(model: &T) -> Option<Vec<u8>> {
    match serde_json::to_vec(model) {
        Ok(mut json) => {
            json.push(b'\n');
            Some(json)
        },
        Err(error) => {
            error!(message = "Failed generating JSON.", %error);
            None
        }
    }
}

#[derive(Debug)]
pub enum NewRelicApiModel {
    Metrics(MetricsApiModel),
    Events(EventsApiModel),
    Logs(LogsApiModel)
}

type KeyValData = HashMap<String, Value>;
type DataStore = HashMap<String, Vec<KeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsApiModel(Vec<DataStore>);

impl MetricsApiModel {
    pub fn new(metric_array: Vec<(Value, Value, Value)>) -> Self {
        let mut metric_data_array = vec!();
        for (m_name, m_value, m_timestamp) in metric_array {
            let mut metric_data = KeyValData::new();
            metric_data.insert("name".to_owned(), m_name);
            metric_data.insert("value".to_owned(), m_value);
            match m_timestamp {
                Value::Timestamp(ts) => { metric_data.insert("timestamp".to_owned(), Value::from(ts.timestamp())); },
                Value::Integer(i) => { metric_data.insert("timestamp".to_owned(), Value::from(i)); },
                _ => { metric_data.insert("timestamp".to_owned(), Value::from(DateTime::<Utc>::from(SystemTime::now()).timestamp())); }
            }
            metric_data_array.push(metric_data);
        }
        let mut metric_store = DataStore::new();
        metric_store.insert("metrics".to_owned(), metric_data_array);
        Self(vec!(metric_store))
    }
}

impl TryFrom<Vec<Event>> for MetricsApiModel {
    type Error = &'static str;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut metric_array = vec!();

        for buf_event in buf_events {
            match buf_event {
                Event::Metric(metric) => {
                    // Future improvement: put metric type. If type = count, NR metric model requiere an interval.ms field, that is not provided by the Vector Metric model.
                    match metric.value() {
                        MetricValue::Gauge { value } => {
                            metric_array.push((
                                Value::from(metric.name().to_owned()),
                                Value::from(*value),
                                Value::from(metric.timestamp())
                            ));
                        },
                        MetricValue::Counter { value } => {
                            metric_array.push((
                                Value::from(metric.name().to_owned()),
                                Value::from(*value),
                                Value::from(metric.timestamp())
                            ));
                        },
                        _ => {
                            // Unrecognized metric type
                        }
                    }
                },
                _ => {
                    // Unrecognized event type
                }
            }
        }

        if metric_array.len() > 0 {
            Ok(MetricsApiModel::new(metric_array))
        }
        else {
            Err("No valid metrics to generate")
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventsApiModel(Vec<KeyValData>);

impl EventsApiModel {
    pub fn new(events_array: Vec<KeyValData>) -> Self {
        Self(events_array)
    }
}

impl TryFrom<Vec<Event>> for EventsApiModel {
    type Error = &'static str;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut events_array = vec!();
        for buf_event in buf_events {
            match buf_event {
                Event::Log(log) => {
                    let mut event_model = KeyValData::new();
                    for (k, v) in log.all_fields() {
                        event_model.insert(k, v.clone());
                    }

                    if let Some(message) = log.get("message") {
                        let message = message.to_string_lossy().replace("\\\"", "\"");
                        // If message contains a JSON string, parse it and insert all fields into self
                        if let serde_json::Result::Ok(json_map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&message) {
                            for (k, v) in json_map {
                                match v {
                                    serde_json::Value::String(s) => {
                                        event_model.insert(k, Value::from(s));
                                    },
                                    serde_json::Value::Number(n) => {
                                        if n.is_f64() {
                                            event_model.insert(k, Value::from(n.as_f64()));
                                        }
                                        else {
                                            event_model.insert(k, Value::from(n.as_i64()));
                                        }
                                    },
                                    serde_json::Value::Bool(b) => {
                                        event_model.insert(k, Value::from(b));
                                    },
                                    _ => {}
                                }
                            }
                            event_model.remove("message");
                        }
                    }

                    if let None = event_model.get("eventType") {
                        event_model.insert("eventType".to_owned(), Value::from("VectorSink".to_owned()));
                    }

                    events_array.push(event_model);
                },
                _ => {
                    // Unrecognized event type
                }
            }
        }

        if events_array.len() > 0 {
            Ok(Self::new(events_array))
        }
        else {
            Err("No valid events to generate")
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogsApiModel(Vec<DataStore>);

impl LogsApiModel {
    pub fn new(logs_array: Vec<KeyValData>) -> Self {
        let mut logs_store = DataStore::new();
        logs_store.insert("logs".to_owned(), logs_array);
        Self(vec!(logs_store))
    }
}

impl TryFrom<Vec<Event>> for LogsApiModel {
    type Error = &'static str;

    fn try_from(buf_events: Vec<Event>) -> Result<Self, Self::Error> {
        let mut logs_array = vec!();
        for buf_event in buf_events {
            match buf_event {
                Event::Log(log) => {
                    let mut log_model = KeyValData::new();
                    for (k, v) in log.all_fields() {
                        log_model.insert(k, v.clone());
                    }
                    if let None = log.get("message") {
                        log_model.insert("message".to_owned(), Value::from("log from vector".to_owned()));
                    }
                    logs_array.push(log_model);
                },
                _ => {
                    // Unrecognized event type
                }
            }
        }

        if logs_array.len() > 0 {
            Ok(Self::new(logs_array))
        }
        else {
            Err("No valid logs to generate")
        }
    }
}

#[derive(Debug)]
pub struct NewRelicSinkError {
    message: String
}

impl NewRelicSinkError {
    pub fn new(msg: &str) -> Self {
        NewRelicSinkError {
            message: String::from(msg)
        }
    }

    pub fn boxed(msg: &str) -> Box<Self> {
        Box::new(
            NewRelicSinkError {
                message: String::from(msg)
            }
        )
    }
}

impl std::fmt::Display for NewRelicSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for NewRelicSinkError {
    fn description(&self) -> &str {
        &self.message
    }
}

#[derive(Debug)]
pub struct NewRelicBuffer {
    buffer: Vec<Event>,
    max_size: BatchSize<Self>
}

impl NewRelicBuffer {
    pub const fn new(max_size: BatchSize<Self>) -> Self {
        Self {
            buffer: Vec::new(),
            max_size
        }
    }
}

impl Batch for NewRelicBuffer {
    type Input = Event;
    type Output = Vec<Event>;

    fn get_settings_defaults(
        _config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(defaults)
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        self.buffer.push(item);
        PushResult::Ok(self.buffer.len() > self.max_size.events)
    }

    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(self.max_size.clone())
    }

    fn finish(self) -> Self::Output {
        self.buffer
    }

    fn num_items(&self) -> usize {
        self.buffer.len()
    }
}

inventory::submit! {
    SinkDescription::new::<NewRelicConfig>("new_relic")
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct NewRelicConfig {
    pub license_key: String,
    pub account_id: String,
    pub region: Option<NewRelicRegion>,
    pub api: NewRelicApi,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigFixed<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig
}

impl_generate_config_from_default!(NewRelicConfig);

#[derive(Debug, Clone)]
pub struct NewRelicApiRequest {
    pub batch_size: usize,
    pub finalizers: EventFinalizers,
    pub credentials: Arc<NewRelicCredentials>,
    pub payload: Vec<u8>,
    pub compression: Compression
}

impl Ackable for NewRelicApiRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for NewRelicApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug, Clone)]
pub struct NewRelicApiService {
    client: HttpClient
}

#[derive(Debug)]
pub enum NewRelicApiResponse {
    Ok,
    NotOk,
}

impl AsRef<EventStatus> for NewRelicApiResponse {
    fn as_ref(&self) -> &EventStatus {
        match self {
            NewRelicApiResponse::Ok => &EventStatus::Delivered,
            NewRelicApiResponse::NotOk => &EventStatus::Errored,
        }
    }
}

impl Service<NewRelicApiRequest> for NewRelicApiService {
    type Response = NewRelicApiResponse;
    type Error = NewRelicSinkError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: NewRelicApiRequest) -> Self::Future {

        info!("Sending {} events.", request.batch_size);

        let mut client = self.client.clone();

        let uri = request.credentials.get_uri();

        let http_request = Request::post(&uri)
            .header(CONTENT_TYPE, "application/json")
            .header("Api-Key", request.credentials.license_key.clone());

        let http_request = if let Some(ce) = request.compression.content_encoding() {
            http_request.header(CONTENT_ENCODING, ce)
        } else {
            http_request
        };

        let http_request = http_request
            .header(CONTENT_LENGTH, request.payload.len())
            .body(Body::from(request.payload))
            .expect("building HTTP request failed unexpectedly");

        Box::pin(async move {
            match client.call(http_request).in_current_span().await {
                Ok(_) => Ok(NewRelicApiResponse::Ok),
                Err(_) => Err(NewRelicSinkError::new("HTTP request error"))
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct NewRelicCredentials {
    pub license_key: String,
    pub account_id: String,
    pub api: NewRelicApi,
    pub region: NewRelicRegion
}

impl NewRelicCredentials {
    pub fn get_uri(&self) -> Uri {
        match self.api {
            NewRelicApi::Events => {
                match self.region {
                    NewRelicRegion::Us => format!("https://insights-collector.newrelic.com/v1/accounts/{}/events", self.account_id).parse::<Uri>().unwrap(),
                    NewRelicRegion::Eu => format!("https://insights-collector.eu01.nr-data.net/v1/accounts/{}/events", self.account_id).parse::<Uri>().unwrap(),
                }
            },
            NewRelicApi::Metrics => {
                match self.region {
                    NewRelicRegion::Us => Uri::from_static("https://metric-api.newrelic.com/metric/v1"),
                    NewRelicRegion::Eu => Uri::from_static("https://metric-api.eu.newrelic.com/metric/v1"),
                }
            },
            NewRelicApi::Logs => {
                match self.region {
                    NewRelicRegion::Us => Uri::from_static("https://log-api.newrelic.com/log/v1"),
                    NewRelicRegion::Eu => Uri::from_static("https://log-api.eu.newrelic.com/log/v1"),
                }
            }
        }
    }
}

impl From<&NewRelicConfig> for NewRelicCredentials {
    fn from(config: &NewRelicConfig) -> Self {
        Self {
            license_key: config.license_key.clone(),
            account_id: config.account_id.clone(),
            api: config.api.clone(),
            region: config.region.unwrap_or(NewRelicRegion::Us)
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "new_relic")]
impl SinkConfig for NewRelicConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let encoding = self.encoding.clone();

        let batcher_settings = BatchSettings::<NewRelicBuffer>::default()
            .events(self.batch.max_events.unwrap_or(50))
            .timeout(self.batch.timeout_secs.unwrap_or(30))
            .parse_config(self.batch)?
            .into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&Default::default());
        let tls_settings = TlsSettings::from_options(&None)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let service = ServiceBuilder::new()
            .settings(request_limits, NewRelicApiRetry)
            .service(NewRelicApiService {
                client
            });

        let credentials = Arc::from(NewRelicCredentials::from(self));

        let sink = NewRelicSink {
            service,
            acker: cx.acker(),
            encoding,
            credentials,
            compression: self.compression,
            batcher_settings,
        };

        Ok((
            super::VectorSink::Stream(Box::new(sink)),
            future::ok(()).boxed(),
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "new_relic"
    }
}

#[derive(Debug, Default, Clone)]
pub struct NewRelicApiRetry;

impl RetryLogic for NewRelicApiRetry {
    type Error = NewRelicSinkError;
    type Response = NewRelicApiResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        // Never retry.
        false
    }
}

struct NewRelicRequestBuilder {
    encoding: EncodingConfigFixed<Encoding>,
    compression: Compression,
    credentials: Arc<NewRelicCredentials>
}

impl RequestBuilder<Vec<Event>> for NewRelicRequestBuilder {
    type Metadata = (Arc<NewRelicCredentials>, usize, EventFinalizers);
    type Events = Result<NewRelicApiModel, &'static str>;
    type Encoder = EncodingConfigFixed<Encoding>;
    type Payload = Vec<u8>;
    type Request = NewRelicApiRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, mut input: Vec<Event>) -> (Self::Metadata, Self::Events) {
        let events_len = input.len();
        let finalizers = input.take_finalizers();
        let api_model = || -> Result<NewRelicApiModel, &str> {
            match self.credentials.api {
                NewRelicApi::Events => {
                    Ok(
                        NewRelicApiModel::Events(
                            EventsApiModel::try_from(input)?
                        )
                    )
                },
                NewRelicApi::Metrics => {
                    Ok(
                        NewRelicApiModel::Metrics(
                            MetricsApiModel::try_from(input)?
                        )
                    )
                },
                NewRelicApi::Logs => {
                    Ok(
                        NewRelicApiModel::Logs(
                            LogsApiModel::try_from(input)?
                        )
                    )
                },
            }
        }();
        let metadata = (self.credentials.clone(), events_len, finalizers);
        (metadata, api_model)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (_credentials, events_len, finalizers) = metadata;
        NewRelicApiRequest {
            batch_size: events_len,
            finalizers,
            credentials: self.credentials.clone(),
            payload,
            compression: self.compression
        }
    }
}

struct NewRelicSink<S> {
    pub service: S,
    pub acker: Acker,
    pub encoding: EncodingConfigFixed<Encoding>,
    pub credentials: Arc<NewRelicCredentials>,
    pub compression: Compression,
    pub batcher_settings: BatcherSettings,
}

impl<S> NewRelicSink<S>
where
    S: Service<NewRelicApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = NullPartitioner::new();
        let builder_limit = NonZeroUsize::new(64);
        let request_builder = NewRelicRequestBuilder {
            encoding: self.encoding,
            compression: self.compression,
            credentials: self.credentials.clone()
        };

        let sink = input
            .batched(partitioner, self.batcher_settings)
            .map(|(_, batch)| batch)
            .request_builder(builder_limit, request_builder)
            .filter_map(|request: Result<NewRelicApiRequest, std::io::Error>| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build New Relic request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink for NewRelicSink<S>
where
    S: Service<NewRelicApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{Event, Value, LogEvent, Metric, MetricKind, MetricValue}
    };
    use std::{
        collections::HashMap,
        convert::TryFrom,
        time::SystemTime
    };
    use chrono::{DateTime, Utc};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NewRelicConfig>();
    }

    #[test]
    fn generate_event_api_model() {
        // Without message field
        let mut map = HashMap::<String, Value>::new();
        map.insert("eventType".to_owned(), Value::from("TestEvent".to_owned()));
        map.insert("user".to_owned(), Value::from("Joe".to_owned()));
        map.insert("user_id".to_owned(), Value::from(123456));
        let event = Event::Log(LogEvent::from(map));
        let model = EventsApiModel::try_from(vec!(event)).expect("Failed mapping events into API model");

        assert_eq!(model.0.len(), 1);
        assert_eq!(model.0[0].get("eventType").is_some(), true);
        assert_eq!(model.0[0].get("eventType").unwrap().to_string_lossy(), "TestEvent".to_owned());
        assert_eq!(model.0[0].get("user").is_some(), true);
        assert_eq!(model.0[0].get("user").unwrap().to_string_lossy(), "Joe".to_owned());
        assert_eq!(model.0[0].get("user_id").is_some(), true);
        assert_eq!(model.0[0].get("user_id").unwrap(), &Value::Integer(123456));

        // With message field
        let mut map = HashMap::<String, Value>::new();
        map.insert("eventType".to_owned(), Value::from("TestEvent".to_owned()));
        map.insert("user".to_owned(), Value::from("Joe".to_owned()));
        map.insert("user_id".to_owned(), Value::from(123456));
        map.insert("message".to_owned(), Value::from("This is a message".to_owned()));
        let event = Event::Log(LogEvent::from(map));
        let model = EventsApiModel::try_from(vec!(event)).expect("Failed mapping events into API model");

        assert_eq!(model.0.len(), 1);
        assert_eq!(model.0[0].get("eventType").is_some(), true);
        assert_eq!(model.0[0].get("eventType").unwrap().to_string_lossy(), "TestEvent".to_owned());
        assert_eq!(model.0[0].get("user").is_some(), true);
        assert_eq!(model.0[0].get("user").unwrap().to_string_lossy(), "Joe".to_owned());
        assert_eq!(model.0[0].get("user_id").is_some(), true);
        assert_eq!(model.0[0].get("user_id").unwrap(), &Value::Integer(123456));
        assert_eq!(model.0[0].get("message").is_some(), true);
        assert_eq!(model.0[0].get("message").unwrap().to_string_lossy(), "This is a message".to_owned());

        // With a JSON encoded inside the message field
        let mut map = HashMap::<String, Value>::new();
        map.insert("eventType".to_owned(), Value::from("TestEvent".to_owned()));
        map.insert("user".to_owned(), Value::from("Joe".to_owned()));
        map.insert("user_id".to_owned(), Value::from(123456));
        map.insert("message".to_owned(), Value::from("{\"my_key\" : \"my_value\"}".to_owned()));
        let event = Event::Log(LogEvent::from(map));
        let model = EventsApiModel::try_from(vec!(event)).expect("Failed mapping events into API model");

        assert_eq!(model.0.len(), 1);
        assert_eq!(model.0[0].get("eventType").is_some(), true);
        assert_eq!(model.0[0].get("eventType").unwrap().to_string_lossy(), "TestEvent".to_owned());
        assert_eq!(model.0[0].get("user").is_some(), true);
        assert_eq!(model.0[0].get("user").unwrap().to_string_lossy(), "Joe".to_owned());
        assert_eq!(model.0[0].get("user_id").is_some(), true);
        assert_eq!(model.0[0].get("user_id").unwrap(), &Value::Integer(123456));
        assert_eq!(model.0[0].get("my_key").is_some(), true);
        assert_eq!(model.0[0].get("my_key").unwrap().to_string_lossy(), "my_value".to_owned());
    }

    #[test]
    fn generate_log_api_model() {
        // Without message field
        let mut map = HashMap::<String, Value>::new();
        map.insert("tag_key".to_owned(), Value::from("tag_value".to_owned()));
        let event = Event::Log(LogEvent::from(map));
        let model = LogsApiModel::try_from(vec!(event)).expect("Failed mapping logs into API model");
        let logs = model.0[0].get("logs").expect("Logs data store not present");

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].get("tag_key").is_some(), true);
        assert_eq!(logs[0].get("tag_key").unwrap().to_string_lossy(), "tag_value".to_owned());
        assert_eq!(logs[0].get("message").is_some(), true);

        // With message field
        let mut map = HashMap::<String, Value>::new();
        map.insert("tag_key".to_owned(), Value::from("tag_value".to_owned()));
        map.insert("message".to_owned(), Value::from("This is a message".to_owned()));
        let event = Event::Log(LogEvent::from(map));
        let model = LogsApiModel::try_from(vec!(event)).expect("Failed mapping logs into API model");
        let logs = model.0[0].get("logs").expect("Logs data store not present");

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].get("tag_key").is_some(), true);
        assert_eq!(logs[0].get("tag_key").unwrap().to_string_lossy(), "tag_value".to_owned());
        assert_eq!(logs[0].get("message").is_some(), true);
        assert_eq!(logs[0].get("message").unwrap().to_string_lossy(), "This is a message".to_owned());
    }

    #[test]
    fn generate_metric_api_model() {
        // Without timestamp
        let event = Event::Metric(Metric::new("my_metric", MetricKind::Absolute, MetricValue::Counter { value: 100.0 }));
        let model = MetricsApiModel::try_from(vec!(event)).expect("Failed mapping metrics into API model");
        let metrics = model.0[0].get("metrics").expect("Logs data store not present");

        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].get("name").is_some(), true);
        assert_eq!(metrics[0].get("name").unwrap().to_string_lossy(), "my_metric".to_owned());
        assert_eq!(metrics[0].get("value").is_some(), true);
        assert_eq!(metrics[0].get("value").unwrap(), &Value::Float(100.0));
        assert_eq!(metrics[0].get("timestamp").is_some(), true);

        // With timestamp
        let m = Metric::new(
            "my_metric",
            MetricKind::Absolute,
            MetricValue::Counter {
                value: 100.0
            }
        ).with_timestamp(Some(DateTime::<Utc>::from(SystemTime::now())));
        let event = Event::Metric(m);
        let model = MetricsApiModel::try_from(vec!(event)).expect("Failed mapping metrics into API model");
        let metrics = model.0[0].get("metrics").expect("Logs data store not present");

        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].get("name").is_some(), true);
        assert_eq!(metrics[0].get("name").unwrap().to_string_lossy(), "my_metric".to_owned());
        assert_eq!(metrics[0].get("value").is_some(), true);
        assert_eq!(metrics[0].get("value").unwrap(), &Value::Float(100.0));
        assert_eq!(metrics[0].get("timestamp").is_some(), true);
    }
}
