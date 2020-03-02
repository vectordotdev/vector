use crate::{
    sinks::splunk_hec::{Encoding, HecSinkConfig},
    sinks::util::{BatchBytesConfig, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use serde::{Deserialize, Serialize};

const HOST: &str = "https://cloud.humio.com";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HumioLogsConfig {
    token: String,
    host: Option<String>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<HumioLogsConfig>("humio_logs")
}

#[typetag::serde(name = "humio")]
impl SinkConfig for HumioLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let host = self.host.clone().unwrap_or_else(|| HOST.to_string());

        HecSinkConfig {
            token: self.token.clone(),
            host,
            encoding: Encoding::Json,
            batch: self.batch.clone(),
            request: self.request.clone(),
            ..Default::default()
        }
        .build(cx)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "humio_logs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::test::load_sink;

    #[test]
    fn smoke() {
        load_sink::<HumioLogsConfig>(
            r#"
            token = "alsdkfjaslkdfjsalkfj"
            host = "https://127.0.0.1"
        "#,
        )
        .unwrap();
    }
}
