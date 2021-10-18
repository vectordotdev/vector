use vector_core::ByteSizeOf;

use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value, Metric, MetricValue, LogEvent},
    http::{HttpClient},
    sinks::util::{
        batch::{BatchError, BatchSize},
        encoding::{EncodingConfigWithDefault, EncodingConfiguration, TimestampFormat},
        http::{BatchedHttpSink, HttpSink},
        Batch, PushResult, BatchConfig, BatchSettings, Compression, TowerRequestConfig,
    },
    tls::{TlsOptions, TlsSettings},
};

use futures::{future, FutureExt, SinkExt};
use http::{Request, Uri};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::TryFrom
};

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicRegion {
    #[derivative(Default)]
    Us,
    Eu,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicApi {
    #[derivative(Default)]
    Events,
    Metrics,
    Logs
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct NewRelicConfig {
    pub license_key: String,
    pub account_id: String,
    pub region: Option<NewRelicRegion>,
    pub api: NewRelicApi,
    //#[serde(default)]
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>
}

pub trait ToJSON<T> : Serialize + TryFrom<T>
where
    <Self as TryFrom<T>>::Error: std::fmt::Display
{
    fn to_json(event: T) -> Option<Vec<u8>> {
        match Self::try_from(event) {
            Ok(model) => {
                match serde_json::to_vec(&model) {
                    Ok(mut json) => {
                        json.push(b'\n');
                        Some(json)
                    },
                    Err(error) => {
                        error!(message = "Failed generating JSON.", %error);
                        None
                    }
                }
            },
            Err(error) => {
                error!(message = "Failed converting model.", %error);
                None
            }
        }
    } 
}

type KeyValData = HashMap<String, Value>;
type DataStore = HashMap<String, Vec<KeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct MetricsApiModel(Vec<DataStore>);

impl MetricsApiModel {
    pub fn new(metric_array: Vec<(Value, Value, Value, Value)>) -> Self {
        let mut metric_data_array = vec!();
        for (m_name, m_type, m_value, m_timestamp) in metric_array {
            let mut metric_data = KeyValData::new();
            metric_data.insert("name".to_owned(), m_name);
            metric_data.insert("type".to_owned(), m_type);
            metric_data.insert("value".to_owned(), m_value);
            match m_timestamp {
                Value::Timestamp(ts) => { metric_data.insert("timestamp".to_owned(), Value::from(ts.timestamp())); },
                Value::Integer(i) => { metric_data.insert("timestamp".to_owned(), Value::from(i)); },
                _ => {}
            }
            metric_data_array.push(metric_data);
        }
        let mut metric_store = DataStore::new();
        metric_store.insert("metrics".to_owned(), metric_data_array);
        Self(vec!(metric_store))
    }
}

impl ToJSON<Vec<BufEvent>> for MetricsApiModel {}

impl TryFrom<Vec<BufEvent>> for MetricsApiModel {
    type Error = &'static str;

    fn try_from(buf_events: Vec<BufEvent>) -> Result<Self, Self::Error> {
        let mut metric_array = vec!();

        for buf_event in buf_events {
            match buf_event {
                BufEvent::Metric(metric) => {
                    match metric.value() {
                        MetricValue::Gauge { value } => {
                            metric_array.push((
                                Value::from(metric.name().to_owned()),
                                Value::from("gauge".to_owned()),
                                Value::from(*value),
                                //TODO: check Some instead of unwraping
                                Value::from(metric.timestamp().unwrap())
                            ));
                        },
                        MetricValue::Counter { value } => {
                            metric_array.push((
                                Value::from(metric.name().to_owned()),
                                Value::from("count".to_owned()),
                                Value::from(*value),
                                //TODO: check Some instead of unwraping
                                Value::from(metric.timestamp().unwrap())
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

impl ToJSON<Vec<BufEvent>> for EventsApiModel {}

impl TryFrom<Vec<BufEvent>> for EventsApiModel {
    type Error = &'static str;

    fn try_from(buf_events: Vec<BufEvent>) -> Result<Self, Self::Error> {
        let mut events_array = vec!();
        for buf_event in buf_events {
            match buf_event {
                BufEvent::Log(log) => {
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

impl ToJSON<Vec<BufEvent>> for LogsApiModel {}

impl TryFrom<Vec<BufEvent>> for LogsApiModel {
    type Error = &'static str;

    fn try_from(buf_events: Vec<BufEvent>) -> Result<Self, Self::Error> {
        let mut logs_array = vec!();
        for buf_event in buf_events {
            match buf_event {
                BufEvent::Log(log) => {
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

//TODO: rename NewRelicSample, contain models of New Relic Event, Log and Metric ionstead of Vector models.
//TODO: or even better, remove this model and use Vector's OOTB Event instead.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum BufEvent {
    Log(LogEvent),
    Metric(Metric),
}

impl BufEvent {
    pub fn remap(event: Event) -> Self {
        match event {
            Event::Log(log) => Self::Log(log),
            Event::Metric(metric) => Self::Metric(metric)
        }
    }
}

impl ByteSizeOf for BufEvent {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Log(_) => std::mem::size_of::<LogEvent>(),
            Self::Metric(_) => std::mem::size_of::<Metric>()
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
    buffer: Vec<BufEvent>,
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
    type Input = BufEvent;
    type Output = Vec<BufEvent>;

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

impl_generate_config_from_default!(NewRelicConfig);

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

#[async_trait::async_trait]
#[typetag::serde(name = "new_relic")]
impl SinkConfig for NewRelicConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {

        let batch = BatchSettings::<NewRelicBuffer>::default()
            .events(5)
            .timeout(5)
            .parse_config(self.batch)?;
        let request = self.request.unwrap_with(&TowerRequestConfig::default());
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            //Buffer::new(batch.size, self.compression),
            NewRelicBuffer::new(batch.size),
            request,
            batch.timeout,
            client.clone(),
            cx.acker()
        )
        .sink_map_err(|error| error!(message = "Fatal new_relic sink error.", %error));

        Ok((
            super::VectorSink::Sink(Box::new(sink)),
            future::ok(()).boxed()
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "new_relic"
    }
}

#[async_trait::async_trait]
impl HttpSink for NewRelicConfig {
    type Input = BufEvent;
    type Output = Vec<BufEvent>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let encoding = EncodingConfigWithDefault {
            timestamp_format: Some(TimestampFormat::Unix),
            ..self.encoding.clone()
        };
        encoding.apply_rules(&mut event);

        //TODO: remove this before production
        println!("------------------------------------------------------------------------");
        println!("Encode event =\n{:#?}", event);
        println!("------------------------------------------------------------------------");

        Some(BufEvent::remap(event))

        //TODO: buffer event

        //TODO: if buffer is full, generate JSON and return it, otherwise return None

        /*
        match self.api {
            NewRelicApi::Events => {
                if let Event::Log(log) = event {
                    NewRelicEvent::to_json(log)
                }
                else {
                    info!("Received Metric while expecting events, ignoring");
                    None
                }
            },
            NewRelicApi::Metrics => {
                if let Event::Metric(metric) = event {
                    NewRelicMetric::to_json(metric)
                }
                else {
                    info!("Received LogEvent while expecting metrics, ignoring");
                    None
                }
            },
            NewRelicApi::Logs => {
                if let Event::Log(log) = event {
                    NewRelicLog::to_json(log)
                }
                else {
                    info!("Received Metric while expecting logs, ignoring");
                    None
                }
            }
        }
        */
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {

        println!("------------------------------------------------------------------------");
        println!("Build request events =\n{:#?}", events);
        println!("------------------------------------------------------------------------");

        let uri = match self.api {
            NewRelicApi::Events => {
                match self.region.as_ref().unwrap_or(&NewRelicRegion::Us) {
                    NewRelicRegion::Us => Uri::from_static("http://localhost:8888/events/us"),
                    NewRelicRegion::Eu => Uri::from_static("http://localhost:8888/events/eu"),
                    /*
                    NewRelicRegion::Us => format!("https://insights-collector.newrelic.com/v1/accounts/{}/events", self.account_id).parse::<Uri>().unwrap(),
                    NewRelicRegion::Eu => format!("https://insights-collector.eu01.nr-data.net/v1/accounts/{}/events", self.account_id).parse::<Uri>().unwrap(),
                    */
                }
            },
            NewRelicApi::Metrics => {
                match self.region.as_ref().unwrap_or(&NewRelicRegion::Us) {
                    NewRelicRegion::Us => Uri::from_static("http://localhost:8888/metrics/us"),
                    NewRelicRegion::Eu => Uri::from_static("http://localhost:8888/metrics/eu"),
                    /*
                    NewRelicRegion::Us => Uri::from_static("https://metric-api.newrelic.com/metric/v1"),
                    NewRelicRegion::Eu => Uri::from_static("https://metric-api.eu.newrelic.com/metric/v1"),
                    */
                }
            },
            NewRelicApi::Logs => {
                match self.region.as_ref().unwrap_or(&NewRelicRegion::Us) {
                    NewRelicRegion::Us => Uri::from_static("http://localhost:8888/logs/us"),
                    NewRelicRegion::Eu => Uri::from_static("http://localhost:8888/logs/eu"),
                    /*
                    NewRelicRegion::Us => Uri::from_static("https://log-api.newrelic.com/log/v1"),
                    NewRelicRegion::Eu => Uri::from_static("https://log-api.eu.newrelic.com/log/v1"),
                    */
                }
            }
        };

        let json = match self.api {
            NewRelicApi::Metrics => MetricsApiModel::to_json(events),
            NewRelicApi::Logs => LogsApiModel::to_json(events),
            NewRelicApi::Events => EventsApiModel::to_json(events)
        };

        if let Some(json) = json {
            let mut builder = Request::post(&uri).header("Content-Type", "application/json");
            builder = builder.header("Api-Key", self.license_key.clone());

            if let Some(ce) = self.compression.content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            let request = builder.body(json).unwrap();

            Ok(request)
        }
        else {
            Err(NewRelicSinkError::boxed("Error generating API model"))
        }
    }
}

//TODO: tests