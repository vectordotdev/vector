use snafu::Snafu;
use vector_config_macros::configurable_component;
use vector_lib::tls::{TlsEnableableConfig, TlsError};

use crate::template::TemplateParseError;

/// Shared MQTT configuration for sources and sinks.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct MqttCommonConfig {
    /// MQTT server address (The broker’s domain name or IP address).
    #[configurable(metadata(docs::examples = "mqtt.example.com", docs::examples = "127.0.0.1"))]
    pub host: String,

    /// TCP port of the MQTT server to connect to.
    #[configurable(derived)]
    #[serde(default = "default_port")]
    #[derivative(Default(value = "default_port()"))]
    pub port: u16,

    /// MQTT username.
    #[serde(default)]
    #[configurable(derived)]
    pub user: Option<String>,

    /// MQTT password.
    #[serde(default)]
    #[configurable(derived)]
    pub password: Option<String>,

    /// MQTT client ID.
    #[serde(default)]
    #[configurable(derived)]
    pub client_id: Option<String>,

    /// Connection keep-alive interval.
    #[serde(default = "default_keep_alive")]
    #[derivative(Default(value = "default_keep_alive()"))]
    pub keep_alive: u16,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,
}

const fn default_port() -> u16 {
    1883
}

const fn default_keep_alive() -> u16 {
    60
}

/// MQTT Error Types
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum MqttError {
    /// Topic template parsing failed
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate {
        /// Source of error
        source: TemplateParseError,
    },
    /// TLS error
    #[snafu(display("TLS error: {}", source))]
    Tls {
        /// Source of error
        source: TlsError,
    },
    /// Configuration error
    #[snafu(display("MQTT configuration error: {}", source))]
    Configuration {
        /// Source of error
        source: ConfigurationError,
    },
}

/// MQTT Configuration error types
#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum ConfigurationError {
    /// Empty client ID error
    #[snafu(display("Client ID is not allowed to be empty."))]
    EmptyClientId,
    /// Invalid credentials provided error
    #[snafu(display("Username and password must be either both provided or both missing."))]
    InvalidCredentials,
    /// Invalid client ID provied error
    #[snafu(display(
        "Client ID must be 1-23 characters long and must consist of only alphanumeric characters."
    ))]
    InvalidClientId,
    /// Credentials provided were imcomplete
    #[snafu(display("Username and password must be either both or neither provided."))]
    IncompleteCredentials,
}
