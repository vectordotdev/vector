use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value, Metric, MetricValue, LogEvent},
    http::{HttpClient},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration, TimestampFormat},
        http::{BatchedHttpSink, HttpSink},
        BatchConfig, BatchSettings, Buffer, Compression, TowerRequestConfig,
    },
    tls::{TlsOptions, TlsSettings},
};

use futures::{future, FutureExt, SinkExt};
use http::{Request, Uri};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub tls: Option<TlsOptions>,
}

pub trait ToJSON : Serialize {
    fn to_json(&self) -> Option<Vec<u8>> {
        let mut json = serde_json::to_vec(self).ok()?;
        json.push(b'\n');
        Some(json)
    }
}

type NRKeyValData = HashMap<String, Value>;
type NRMetricStore = HashMap<String, Vec<NRKeyValData>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct NewRelicMetric(Vec<NRMetricStore>);

impl NewRelicMetric {
    pub fn new(m_name: Value, m_type: Value, m_value: Value) -> Self {
        let mut metric_data = NRKeyValData::new();
        metric_data.insert("name".to_owned(), m_name);
        metric_data.insert("type".to_owned(), m_type);
        metric_data.insert("value".to_owned(), m_value);
        let mut metric_store = NRMetricStore::new();
        metric_store.insert("metrics".to_owned(), vec!(metric_data));
        Self(vec!(metric_store))
    }
    
    pub fn json_from_metric(metric: Metric) -> Option<Vec<u8>> {
        match metric.value() {
            MetricValue::Gauge { value } => {
                Self::new(
                    Value::from(metric.name().to_owned()),
                    Value::from("gauge".to_owned()),
                    Value::from(*value)
                ).to_json()
            },
            MetricValue::Counter { value } => {
                Self::new(
                    Value::from(metric.name().to_owned()),
                    Value::from("count".to_owned()),
                    Value::from(*value)
                ).to_json()
            },
            _ => {
                None
            }
        }
    }

    pub fn json_from_log(log: LogEvent) -> Option<Vec<u8>> {
        if let Some(m_name) = log.get("name") {
            if let Some(m_value) = log.get("value") {
                if let Some(m_type) = log.get("type") {
                    return Self::new(m_name.clone(), m_type.clone(), m_value.clone()).to_json();
                }
            }
        }
        None
    }
}

impl ToJSON for NewRelicMetric {}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewRelicEvent(NRKeyValData);

impl NewRelicEvent {
    pub fn new() -> Self {
        Self(NRKeyValData::new())
    }

    pub fn json_from_log(log: LogEvent) -> Option<Vec<u8>> {
        let mut s = Self::new();
        for (k, v) in log.all_fields() {
            s.0.insert(k, v.clone());
        }
        if let None = log.get("eventType") {
            s.0.insert("eventType".to_owned(), Value::from("VectorSink".to_owned()));
        }
        s.to_json()
    }
}

impl ToJSON for NewRelicEvent {}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewRelicLog(NRKeyValData);

impl NewRelicLog {
    pub fn new() -> Self {
        Self(NRKeyValData::new())
    }

    pub fn json_from_log(log: LogEvent) -> Option<Vec<u8>> {
        let mut s = Self::new();
        for (k, v) in log.all_fields() {
            s.0.insert(k, v.clone());
        }
        if let None = log.get("message") {
            s.0.insert("message".to_owned(), Value::from("log from vector".to_owned()));
        }
        s.to_json()
    }
}

impl ToJSON for NewRelicLog {}

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
        //TODO: for prod usage: increase bytes (~10MB) and timeout (~1 minute), or even put it in the config
        let batch = BatchSettings::default()
            .bytes(bytesize::mb(1u64))
            .timeout(1)
            .parse_config(self.batch)?;
        let request = self.request.unwrap_with(&TowerRequestConfig::default());
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        //Batched sink send blocks of logs, but does the New Relic collector accept this format?
        let sink = BatchedHttpSink::new(
            self.clone(),
            Buffer::new(batch.size, self.compression),
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
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let encoding = EncodingConfigWithDefault {
            timestamp_format: Some(TimestampFormat::Unix),
            ..self.encoding.clone()
        };
        encoding.apply_rules(&mut event);

        match self.api {
            NewRelicApi::Events => {
                if let Event::Log(log) = event {
                    NewRelicEvent::json_from_log(log)
                }
                else {
                    None
                }
            },
            NewRelicApi::Metrics => {
                match event {
                    Event::Log(log) => {
                        NewRelicMetric::json_from_log(log)
                    },
                    Event::Metric(metric) => {
                        NewRelicMetric::json_from_metric(metric)
                    }
                }
            },
            NewRelicApi::Logs => {
                if let Event::Log(log) = event {
                    NewRelicLog::json_from_log(log)
                }
                else {
                    None
                }
            }
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {

        //TODO: set correct URLs
        let uri = match self.api {
            NewRelicApi::Events => {
                match self.region.as_ref().unwrap_or(&NewRelicRegion::Us) {
                    NewRelicRegion::Us => Uri::from_static("http://localhost:8888/events/us"),
                    NewRelicRegion::Eu => Uri::from_static("http://localhost:8888/events/eu"),
                }
            },
            NewRelicApi::Metrics => {
                match self.region.as_ref().unwrap_or(&NewRelicRegion::Us) {
                    NewRelicRegion::Us => Uri::from_static("http://localhost:8888/metrics/us"),
                    NewRelicRegion::Eu => Uri::from_static("http://localhost:8888/metrics/eu"),
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

        let mut builder = Request::post(&uri).header("Content-Type", "application/json");
        //TODO: change it when sending metrics, use "Api-Key" instead
        builder = builder.header("X-License-Key", self.license_key.clone());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        let request = builder.body(events).unwrap();

        Ok(request)
    }
}
