use crate::{
    sinks::splunk_hec::{Encoding, HecSinkConfig},
    sinks::util::{BatchBytesConfig, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use serde::{Deserialize, Serialize};

const HOST: &str = "https://cloud.humio.com";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HumioConfig {
    token: String,
    host: Option<String>,
    encoding: Option<Encoding>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default)]
    batch: BatchBytesConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<HumioConfig>("humio")
}

#[typetag::serde(name = "humio")]
impl SinkConfig for HumioConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let host = self.host.clone().unwrap_or_else(|| HOST.to_string());
        let encoding = self.encoding.clone().unwrap_or(Encoding::Json);

        HecSinkConfig {
            token: self.token.clone(),
            host,
            encoding,
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
        "humio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::test::load_sink;

    #[test]
    fn smoke() {
        load_sink::<HumioConfig>(
            r#"
            token = "alsdkfjaslkdfjsalkfj"
            host = "https://127.0.0.1"
        "#,
        )
        .unwrap();
    }
}
