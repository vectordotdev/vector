use crate::{
    sinks::splunk_hec::{self, HecSinkConfig},
    sinks::util::{encoding::EncodingConfigWithDefault, BatchBytesConfig, TowerRequestConfig},
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
}

impl Into<splunk_hec::Encoding> for Encoding {
    fn into(self) -> splunk_hec::Encoding {
        splunk_hec::Encoding::Json
    }
}

#[typetag::serde(name = "humio_logs")]
impl SinkConfig for HumioLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        if self.encoding.codec != Encoding::Json {
            error!("Using an unsupported encoding for Humio");
        }
        let host = self.host.clone().unwrap_or_else(|| HOST.to_string());

        HecSinkConfig {
            token: self.token.clone(),
            host,
            encoding: EncodingConfigWithDefault {
                codec: self.encoding.codec.clone().into(),
                only_fields: self.encoding.only_fields.clone(),
                except_fields: self.encoding.except_fields.clone(),
                timestamp_format: self.encoding.timestamp_format.clone(),
            },
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
