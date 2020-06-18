use crate::{
    sinks::splunk_hec::{self, HecSinkConfig},
    sinks::util::{
        encoding::EncodingConfigWithDefault, service2::TowerRequestConfig, BatchBytesConfig,
        Compression,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use serde::{Deserialize, Serialize};

const HOST: &str = "https://cloud.humio.com";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HumioLogsConfig {
    token: String,
    host: Option<String>,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub compression: Compression,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<HumioLogsConfig>("humio_logs")
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Json,
    Text,
}

impl From<Encoding> for splunk_hec::Encoding {
    fn from(v: Encoding) -> Self {
        match v {
            Encoding::Json => splunk_hec::Encoding::Json,
            Encoding::Text => splunk_hec::Encoding::Text,
        }
    }
}

#[typetag::serde(name = "humio_logs")]
impl SinkConfig for HumioLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        self.build_hec_config().build(cx)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "humio_logs"
    }
}

impl HumioLogsConfig {
    fn build_hec_config(&self) -> HecSinkConfig {
        let host = self.host.clone().unwrap_or_else(|| HOST.to_string());

        HecSinkConfig {
            token: self.token.clone(),
            host,
            encoding: self.encoding.clone().transmute(),
            compression: self.compression,
            batch: self.batch.clone(),
            request: self.request.clone(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::sinks::util::{http::HttpSink, test::load_sink};
    use chrono::Utc;
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct HecEventJson {
        time: f64,
    }

    #[test]
    fn humio_valid_time_field() {
        let event = Event::from("hello world");

        let (config, _, _) = load_sink::<HumioLogsConfig>(
            r#"
            token = "alsdkfjaslkdfjsalkfj"
            host = "https://127.0.0.1"
        "#,
        )
        .unwrap();
        let config = config.build_hec_config();

        let bytes = config.encode_event(event).unwrap();
        let hec_event = serde_json::from_slice::<HecEventJson>(&bytes[..]).unwrap();

        let now = Utc::now().timestamp_millis() as f64 / 1000f64;
        assert!(
            (hec_event.time - now).abs() < 0.2,
            format!("hec_event.time = {}, now = {}", hec_event.time, now)
        );
        assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
    }
}
