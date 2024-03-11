//! Client timeout configuration for AWS operations.
use std::time::Duration;
use vector_lib::configurable::configurable_component;

use serde_with::serde_as;

/// Client timeout configuration for AWS operations.
#[serde_as]
#[configurable_component]
#[derive(Copy, Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct AwsTimeout {
    /// The connection timeout for AWS requests
    ///
    /// Limits the ammount of time allowed to initiate a socket connection.
    #[serde(default = "default_timeout")]
    #[serde(rename = "connect_timeout_seconds")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    connect_timeout: Duration,

    /// The operation timeout for AWS requests
    ///
    /// Limits the amount of time allowd for an operation to be fully serviced; an operation
    /// represents the full request/response lifecycle of a call to a service.
    #[serde(default = "default_operation_timeout")]
    #[serde(rename = "operation_timeout_seconds")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    operation_timeout: Duration,

    /// The read timeout for AWS requests
    ///
    /// Limits the amount of time allowed to read the first byte of a response from the time the
    /// request is initiated.
    #[serde(default = "default_timeout")]
    #[serde(rename = "read_timeout_seconds")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    read_timeout: Duration,
}

const fn default_timeout() -> Duration {
    Duration::from_secs(20)
}

const fn default_operation_timeout() -> Duration {
    Duration::from_secs(30)
}

impl AwsTimeout {
    /// returns the connection timeout
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout.clone()
    }

    /// returns the operation timeout
    pub fn operation_timeout(&self) -> Duration {
        self.operation_timeout.clone()
    }

    /// returns the read timeout
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_timeout_default() {
        let config = toml::from_str::<AwsTimeout>(
            r#"
        "#,
        )
        .unwrap();

        assert_eq!(config.connect_timeout, Duration::from_secs(20));
        assert_eq!(config.operation_timeout, Duration::from_secs(30));
        assert_eq!(config.read_timeout, Duration::from_secs(20));
    }

    #[test]
    fn parsing_timeout_configuration() {
        let config = toml::from_str::<AwsTimeout>(
            r#"
            connect_timeout_seconds = 20
            operation_timeout_seconds = 20
            read_timeout_seconds = 60
        "#,
        )
        .unwrap();

        assert_eq!(config.connect_timeout, Duration::from_secs(20));
        assert_eq!(config.operation_timeout, Duration::from_secs(20));
        assert_eq!(config.read_timeout, Duration::from_secs(60));
    }
}
