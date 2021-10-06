use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value},
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
                if let Event::Log(mut log) = event {
                    if let None = log.get("eventType") {
                        log.insert("eventType", Value::from(String::from("VectorSink")));
                    }
                    let mut body = serde_json::to_vec(&log).expect("Events should be valid json!");
                    body.push(b'\n');
                    Some(body)
                }
                else {
                    None
                }
            },
            NewRelicApi::Metrics => {
                //TODO: For Metrics, check name and valu exist and has correct type. Also check type has a valid value if exist
                //TODO: For metrics, remove host, message and source_type
                /*
                log.remove("host");
                log.remove("message");
                log.remove("source_type");
                */
                match event {
                    Event::Log(_log) => {
                        //TODO: generate a New Relic Metric model from a Log
                        None
                    },
                    Event::Metric(metric) => {
                        println!("----------> METRIC object received = {:#?}", metric);
                        //TODO: generate a New Relic Metric model from a Metric
                        None
                    }
                }
            },
            NewRelicApi::Logs => {
                if let Event::Log(mut log) = event {
                    let mut body = serde_json::to_vec(&log).expect("Events should be valid json!");
                    body.push(b'\n');
                    Some(body)
                }
                else {
                    None
                }
            }
        }

        /*
        println!("----------> LOG object = {:#?}", log);

        let field = crate::config::log_schema().message_key();
        println!("----------> Get field {}", field);
        let message = log.get(field).expect("Message field not found");
        let message = message.to_string_lossy();

        println!("Message is = {:#?}", message);
        */
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
                    NewRelicRegion::Us => Uri::from_static("https://log-api.newrelic.com/log/v1"),
                    NewRelicRegion::Eu => Uri::from_static("https://log-api.eu.newrelic.com/log/v1"),
                }
            }
        };

        let mut builder = Request::post(&uri).header("Content-Type", "application/json");
        builder = builder.header("X-License-Key", self.license_key.clone());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        let request = builder.body(events).unwrap();

        Ok(request)
    }
}
