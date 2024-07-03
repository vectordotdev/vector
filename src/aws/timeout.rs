//! Client timeout configuration for AWS operations.
//use std::time::Duration;
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
    /// Limits the amount of time allowed to initiate a socket connection.
    #[configurable(metadata(docs::examples = 20))]
    #[configurable(metadata(docs::human_name = "Connect Timeout"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "connect_timeout_seconds")]
    connect_timeout: Option<u64>,

    /// The operation timeout for AWS requests
    ///
    /// Limits the amount of time allowed for an operation to be fully serviced; an
    /// operation represents the full request/response lifecycle of a call to a service.
    /// Take care when configuring this settings to allow enough time for the polling
    /// interval configured in `poll_secs`
    #[configurable(metadata(docs::examples = 20))]
    #[configurable(metadata(docs::human_name = "Operation Timeout"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "operation_timeout_seconds")]
    operation_timeout: Option<u64>,

    /// The read timeout for AWS requests
    ///
    /// Limits the amount of time allowed to read the first byte of a response from the
    /// time the request is initiated. Take care when configuring this settings to allow
    /// enough time for the polling interval configured in `poll_secs`
    #[configurable(metadata(docs::examples = 20))]
    #[configurable(metadata(docs::human_name = "Read Timeout"))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "read_timeout_seconds")]
    read_timeout: Option<u64>,
}

impl AwsTimeout {
    /// returns the connection timeout
    pub const fn connect_timeout(&self) -> Option<u64> {
        self.connect_timeout
    }

    /// returns the operation timeout
    pub const fn operation_timeout(&self) -> Option<u64> {
        self.operation_timeout
    }

    /// returns the read timeout
    pub const fn read_timeout(&self) -> Option<u64> {
        self.read_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(config.connect_timeout, Some(20));
        assert_eq!(config.operation_timeout, Some(20));
        assert_eq!(config.read_timeout, Some(60));
    }
}
