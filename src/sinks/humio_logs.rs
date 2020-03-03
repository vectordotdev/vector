use crate::{
    sinks::splunk_hec::{Encoding, HecSinkConfig},
    sinks::util::{
        encoding::{skip_serializing_if_default, EncodingConfigWithDefault},
        BatchBytesConfig, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use serde::{Deserialize, Serialize};

const HOST: &str = "https://cloud.humio.com";

#[derive(Clone, Debug, Deserialize, Serialize, Derivative)]
pub struct HumioLogsConfig {
    token: String,
    host: Option<String>,
    #[serde(
        deserialize_with = "EncodingConfigWithDefault::from_deserializer",
        skip_serializing_if = "skip_serializing_if_default",
        default = "default_encoding"
    )]
    #[derivative(Default(value = "default_encoding()"))]
    encoding: EncodingConfigWithDefault<Encoding>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<HumioLogsConfig>("humio_logs")
}

fn default_encoding() -> EncodingConfigWithDefault<Encoding> {
    EncodingConfigWithDefault::from(Encoding::Json)
}

#[typetag::serde(name = "humio")]
impl SinkConfig for HumioLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        if self.encoding.codec != Encoding::Json {
            error!("Using an unsupported encoding for Humio");
        }
        let host = self.host.clone().unwrap_or_else(|| HOST.to_string());

        HecSinkConfig {
            token: self.token.clone(),
            host,
            encoding: self.encoding.clone(),
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
