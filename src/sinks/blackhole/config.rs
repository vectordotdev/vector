use std::time::Duration;

use futures::{FutureExt, future};
use serde_with::serde_as;
use vector_lib::configurable::configurable_component;

use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    sinks::{Healthcheck, VectorSink, blackhole::sink::BlackholeSink},
};

const fn default_print_interval_secs() -> Duration {
    Duration::from_secs(0)
}

/// Configuration for the `blackhole` sink.
#[serde_as]
#[configurable_component(sink(
    "blackhole",
    "Send observability events nowhere, which can be useful for debugging purposes."
))]
#[derive(Clone, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct BlackholeConfig {
    /// The interval between reporting a summary of activity.
    ///
    /// Set to `0` (default) to disable reporting.
    #[derivative(Default(value = "default_print_interval_secs()"))]
    #[serde(default = "default_print_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Print Interval"))]
    #[configurable(metadata(docs::examples = 10))]
    pub print_interval_secs: Duration,

    /// The number of events, per second, that the sink is allowed to consume.
    ///
    /// By default, there is no limit.
    #[configurable(metadata(docs::examples = 1000))]
    pub rate: Option<usize>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "blackhole")]
impl SinkConfig for BlackholeConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[async_trait::async_trait]
impl ValidatedSink for BlackholeConfig {
    type Validated = ();

    fn validate(&self) -> crate::Result<()> {
        if self.rate == Some(0) {
            return Err("`rate` must be greater than zero".into());
        }

        Ok(())
    }

    async fn build(
        &self,
        _validated: &(),
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = BlackholeSink::new(self.clone());
        let healthcheck = future::ok(()).boxed();

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }
}

impl GenerateConfig for BlackholeConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self::default()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ValidatedSink;
    use crate::sinks::blackhole::config::BlackholeConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<BlackholeConfig>();
    }

    #[test]
    fn validate_ok() {
        let config = BlackholeConfig::default();
        config.validate().expect("validation should succeed");
    }

    #[test]
    fn validate_rejects_zero_rate() {
        let config = BlackholeConfig {
            rate: Some(0),
            ..Default::default()
        };

        assert!(config.validate().is_err());
    }
}
