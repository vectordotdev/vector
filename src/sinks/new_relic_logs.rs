use crate::{
    buffers::Acker,
    sinks::http::{Encoding, HttpMethod, HttpSinkConfig},
    sinks::util::{BatchConfig, Compression, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkDescription},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicLogsRegion {
    #[derivative(Default)]
    Us,
    Eu,
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct NewRelicLogsConfig {
    pub license_key: Option<String>,
    pub insert_key: Option<String>,
    pub region: Option<NewRelicLogsRegion>,

    #[serde(default, flatten)]
    pub batch: BatchConfig,

    #[serde(flatten)]
    pub request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<NewRelicLogsConfig>("new_relic_logs")
}

#[typetag::serde(name = "new_relic_logs")]
impl SinkConfig for NewRelicLogsConfig {
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let mut headers: IndexMap<String, String> = IndexMap::new();

        if let Some(license_key) = &self.license_key {
            headers.insert("X-License-Key".to_owned(), license_key.clone());
        } else if let Some(insert_key) = &self.insert_key {
            headers.insert("X-Insert-Key".to_owned(), insert_key.clone());
        } else {
            return Err(format!("must provide either 'license_key' or 'insert_key'").into());
        }

        let uri = match self.region.as_ref().unwrap_or(&NewRelicLogsRegion::Us) {
            NewRelicLogsRegion::Us => "https://log-api.newrelic.com/log/v1",
            NewRelicLogsRegion::Eu => "https://log-api.eu.newrelic.com/log/v1",
        };

        let batch_conf = BatchConfig {
            // The max request size is 10MiB, so in order to be comfortably
            // within this we batch up to 5MiB.
            batch_size: Some(
                self.batch
                    .batch_size
                    .unwrap_or(bytesize::mib(5u64) as usize),
            ),
            ..self.batch
        };

        let request_conf = TowerRequestConfig {
            // The default throughput ceiling defaults are relatively
            // conservative so we crank them up for New Relic.
            request_in_flight_limit: Some(self.request.request_in_flight_limit.unwrap_or(100)),
            request_rate_limit_num: Some(self.request.request_rate_limit_num.unwrap_or(100)),
            ..self.request
        };

        let http_conf = HttpSinkConfig {
            uri: uri.to_owned(),
            method: Some(HttpMethod::Post),
            healthcheck_uri: None,
            basic_auth: None,
            headers: Some(headers),
            compression: Some(Compression::None),
            encoding: Encoding::Json,

            batch: batch_conf,
            request: request_conf,

            tls: None,
        };
        http_conf.build(acker)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "new_relic_logs"
    }
}
