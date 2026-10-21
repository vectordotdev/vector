use rumqttc::{AsyncClient, EventLoop, MqttOptions, TlsConfiguration, Transport};
use snafu::{ResultExt, Snafu};
use vector_config_macros::configurable_component;
use vector_lib::tls::{MaybeTlsSettings, TlsEnableableConfig, TlsError};

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
    #[serde(default = "default_port")]
    #[derivative(Default(value = "default_port()"))]
    pub port: u16,

    /// MQTT username.
    #[serde(default)]
    pub user: Option<String>,

    /// MQTT password.
    #[serde(default)]
    pub password: Option<String>,

    /// MQTT client ID.
    #[serde(default)]
    pub client_id: Option<String>,

    /// Connection keep-alive interval.
    #[serde(default = "default_keep_alive")]
    #[derivative(Default(value = "default_keep_alive()"))]
    pub keep_alive: u16,

    /// Maximum packet size
    #[serde(default = "default_max_packet_size")]
    #[derivative(Default(value = "default_max_packet_size()"))]
    pub max_packet_size: usize,

    /// TLS configuration.
    pub tls: Option<TlsEnableableConfig>,
}

const fn default_port() -> u16 {
    1883
}

const fn default_keep_alive() -> u16 {
    60
}

const fn default_max_packet_size() -> usize {
    10 * 1024
}

impl MqttCommonConfig {
    /// Builds the TLS transport from the `tls` options, or returns `None` when TLS is disabled.
    ///
    /// Configured CA certificates replace the system roots, matching Vector's other TLS clients.
    ///
    /// # Errors
    ///
    /// Returns an error if the TLS options are invalid or the TLS connector cannot be built.
    pub fn tls_transport(&self) -> Result<Option<Transport>, MqttError> {
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false).context(TlsSnafu)?;
        let Some(tls) = tls.tls() else {
            return Ok(None);
        };

        let mut builder = native_tls::TlsConnector::builder();
        let mut has_authorities = false;
        for pem in tls.authorities_pem() {
            has_authorities = true;
            let cert = native_tls::Certificate::from_pem(&pem).context(TlsConnectorSnafu)?;
            builder.add_root_certificate(cert);
        }
        builder.disable_built_in_roots(has_authorities);

        if let Some((cert, key)) = tls.identity_pem() {
            let identity =
                native_tls::Identity::from_pkcs8(&cert, &key).context(TlsConnectorSnafu)?;
            builder.identity(identity);
        }

        // Honor the user-configured `tls.alpn_protocols` (e.g. `x-amzn-mqtt-ca`, required to
        // reach AWS IoT Core over port 443), falling back to `mqtt` when it is not set.
        let alpn: Vec<&str> = self
            .tls
            .as_ref()
            .and_then(|tls| tls.options.alpn_protocols.as_ref())
            .filter(|protocols| !protocols.is_empty())
            .map_or_else(
                || vec!["mqtt"],
                |protocols| protocols.iter().map(String::as_str).collect(),
            );
        builder.request_alpns(&alpn);

        let connector = builder.build().context(TlsConnectorSnafu)?;
        Ok(Some(Transport::Tls(TlsConfiguration::NativeConnector(
            connector,
        ))))
    }
}

/// MQTT Error Types
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum MqttError {
    /// Topic template parsing failed
    #[snafu(display("invalid topic template: {source}"))]
    TopicTemplate {
        /// Source of error
        source: TemplateParseError,
    },
    /// TLS error
    #[snafu(display("TLS error: {source}"))]
    Tls {
        /// Source of error
        source: TlsError,
    },
    /// TLS connector error
    #[snafu(display("TLS connector error: {source}"))]
    TlsConnector {
        /// Source of error
        source: native_tls::Error,
    },
    /// Configuration error
    #[snafu(display("MQTT configuration error: {source}"))]
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
    /// Invalid client ID provided error
    #[snafu(display(
        "Client ID must be 1-23 characters long and must consist of only alphanumeric characters."
    ))]
    InvalidClientId,
    /// Credentials provided were incomplete
    #[snafu(display("Username and password must be either both or neither provided."))]
    IncompleteCredentials,
}

#[derive(Clone)]
/// Mqtt connector wrapper
pub struct MqttConnector {
    /// Mqtt connection options
    pub options: MqttOptions,
}

impl MqttConnector {
    /// Creates a new `MqttConnector`
    pub const fn new(options: MqttOptions) -> Self {
        Self { options }
    }

    /// Connects the connector and generates a client and eventloop
    pub fn connect(&self) -> (AsyncClient, EventLoop) {
        let (client, eventloop) = AsyncClient::new(self.options.clone(), 1024);
        (client, eventloop)
    }

    /// TODO: Right now there is no way to implement the healthcheck properly: <https://github.com/bytebeamio/rumqtt/issues/562>
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "Audit and document the existing error contracts separately from lint enforcement."
    )]
    pub fn healthcheck(&self) -> crate::Result<()> {
        Ok(())
    }
}
