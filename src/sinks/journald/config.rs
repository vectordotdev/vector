use futures::{FutureExt, future};
use vector_lib::configurable::configurable_component;

use crate::{
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{Healthcheck, VectorSink, journald::sink::JournaldSink},
};

/// Configuration for the `journald` sink.
#[configurable_component(sink(
    "journald",
    "Send observability events to the systemd journal for local logging."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct JournaldSinkConfig {
    /// Path to the journald socket.
    /// If not specified, the default systemd journal socket will be used.
    #[configurable(metadata(docs::examples = "\"/run/systemd/journal/socket\".to_string()"))]
    #[serde(default = "default_journald_path")]
    pub journald_path: String,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

fn default_journald_path() -> String {
    "/run/systemd/journal/socket".to_string()
}

impl Default for JournaldSinkConfig {
    fn default() -> Self {
        Self {
            journald_path: default_journald_path(),
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

impl GenerateConfig for JournaldSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "journald")]
impl SinkConfig for JournaldSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = JournaldSink::new(self.clone())?;
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<JournaldSinkConfig>();
    }

    #[test]
    fn test_config_default() {
        let config = JournaldSinkConfig::default();
        assert_eq!(
            config.journald_path,
            "/run/systemd/journal/socket".to_string()
        );
    }
}
