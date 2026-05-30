use std::time::Duration;

use rand::Rng;
use rumqttc::v5::mqttbytes::v5::PublishProperties;
use rumqttc::{
    AsyncClient as AsyncClientV3, EventLoop as EventLoopV3, MqttOptions as MqttOptionsV3,
    TlsConfiguration, Transport,
};
use rumqttc::{
    v5::AsyncClient as AsyncClientV5, v5::EventLoop as EventLoopV5,
    v5::MqttOptions as MqttOptionsV5,
};
use snafu::{ResultExt, Snafu};
use vector_config_macros::configurable_component;
use vector_lib::tls::{MaybeTlsSettings, TlsEnableableConfig, TlsError};

use crate::template::TemplateParseError;

/// MQTT protocol version.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MqttProtocolVersion {
    /// MQTT 3.1.1
    #[default]
    V311,

    /// MQTT 5.0
    V5,
}

/// V5 publish properties that can be set on outgoing messages.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct MqttPublishProperties {
    /// Payload format indicator (0 = unspecified bytes, 1 = UTF-8 encoded).
    #[serde(default)]
    #[configurable(derived)]
    pub payload_format_indicator: Option<u8>,

    /// Message expiry interval in seconds.
    #[serde(default)]
    #[configurable(derived)]
    pub message_expiry_interval: Option<u32>,

    /// Topic alias value.
    #[serde(default)]
    #[configurable(derived)]
    pub topic_alias: Option<u16>,

    /// Response topic for request/response pattern.
    #[serde(default)]
    #[configurable(derived)]
    pub response_topic: Option<String>,

    /// Correlation data for request/response pattern.
    ///
    /// This is raw binary data and is encoded as a byte array in configuration.
    #[serde(default)]
    #[configurable(derived)]
    pub correlation_data: Option<Vec<u8>>,

    /// Content type of the payload (e.g. "application/json").
    #[serde(default)]
    #[configurable(derived)]
    pub content_type: Option<String>,

    /// User properties as ordered key-value pairs.
    #[serde(default)]
    #[configurable(derived)]
    pub user_properties: Vec<MqttUserProperty>,
}

impl MqttPublishProperties {
    /// Converts to rumqttc v5 PublishProperties.
    pub fn to_publish_properties(&self) -> Result<PublishProperties, ConfigurationError> {
        if let Some(value) = self.payload_format_indicator
            && !matches!(value, 0 | 1)
        {
            return Err(ConfigurationError::InvalidPayloadFormatIndicator { value });
        }

        Ok(PublishProperties {
            payload_format_indicator: self.payload_format_indicator,
            message_expiry_interval: self.message_expiry_interval,
            topic_alias: self.topic_alias,
            response_topic: self.response_topic.clone(),
            correlation_data: self
                .correlation_data
                .as_ref()
                .map(|data| bytes::Bytes::copy_from_slice(data)),
            content_type: self.content_type.clone(),
            user_properties: self
                .user_properties
                .iter()
                .map(|property| (property.key.clone(), property.value.clone()))
                .collect(),
            subscription_identifiers: Vec::new(),
        })
    }
}

/// MQTT v5 user property preserving duplicate keys and ordering.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MqttUserProperty {
    /// User property key.
    pub key: String,

    /// User property value.
    pub value: String,
}

/// V5 connection-level properties.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct MqttConnectProperties {
    /// Session expiry interval in seconds.
    /// When set to 0, the session ends when the connection is closed.
    #[serde(default)]
    #[configurable(derived)]
    pub session_expiry_interval: Option<u32>,

    /// Maximum number of topic aliases the client accepts from the server.
    #[serde(default)]
    #[configurable(derived)]
    pub topic_alias_max: Option<u16>,

    /// User properties sent on CONNECT.
    #[serde(default)]
    #[configurable(derived)]
    pub user_properties: Vec<MqttUserProperty>,
}

/// Shared MQTT configuration for sources and sinks.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct MqttCommonConfig {
    /// MQTT server address (The broker's domain name or IP address).
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

    /// Maximum packet size.
    #[serde(default = "default_max_packet_size")]
    #[derivative(Default(value = "default_max_packet_size()"))]
    pub max_packet_size: usize,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// MQTT protocol version (v3 or v5).
    #[configurable(derived)]
    #[serde(default)]
    pub protocol_version: MqttProtocolVersion,

    /// MQTT v5 connection properties. Only used when protocol_version is v5.
    #[configurable(derived)]
    #[serde(default)]
    pub connect_properties: Option<MqttConnectProperties>,
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
    /// Invalid v5 payload format indicator
    #[snafu(display(
        "MQTT v5 payload_format_indicator must be either 0 (bytes) or 1 (UTF-8), got {value}."
    ))]
    InvalidPayloadFormatIndicator {
        /// The invalid configured value.
        value: u8,
    },
    /// Credentials provided were incomplete
    #[snafu(display("Username and password must be either both or neither provided."))]
    IncompleteCredentials,
}

/// Protocol-aware MQTT client wrapper.
pub enum MqttClient {
    /// MQTT v3.1.1 client
    V311(AsyncClientV3),
    /// MQTT v5 client
    V5(AsyncClientV5),
}

/// Protocol-aware MQTT event loop wrapper.
pub enum MqttEventLoop {
    /// MQTT v3.1.1 event loop
    V311(Box<EventLoopV3>),
    /// MQTT v5 event loop
    V5(Box<EventLoopV5>),
}

/// MQTT connector wrapper supporting both v3 and v5 protocols.
#[derive(Clone)]
pub struct MqttConnector {
    /// Protocol version
    pub protocol_version: MqttProtocolVersion,
    /// MQTT v3 connection options
    pub options_v3: Option<MqttOptionsV3>,
    /// MQTT v5 connection options
    pub options_v5: Option<MqttOptionsV5>,
}

impl MqttConnector {
    /// Connects the connector and generates a protocol-aware client and event loop.
    pub fn connect(&self) -> (MqttClient, MqttEventLoop) {
        match self.protocol_version {
            MqttProtocolVersion::V311 => {
                let options = self
                    .options_v3
                    .clone()
                    .expect("v3 options must be set for v3 protocol");
                let (client, eventloop) = AsyncClientV3::new(options, 1024);
                (
                    MqttClient::V311(client),
                    MqttEventLoop::V311(Box::new(eventloop)),
                )
            }
            MqttProtocolVersion::V5 => {
                let options = self
                    .options_v5
                    .clone()
                    .expect("v5 options must be set for v5 protocol");
                let (client, eventloop) = AsyncClientV5::new(options, 1024);
                (
                    MqttClient::V5(client),
                    MqttEventLoop::V5(Box::new(eventloop)),
                )
            }
        }
    }

    /// Returns the broker address string for diagnostics.
    pub fn broker_address(&self) -> String {
        match self.protocol_version {
            MqttProtocolVersion::V311 => self.options_v3.as_ref().map_or_else(String::new, |o| {
                let (host, port) = o.broker_address();
                format!("{host}:{port}")
            }),
            MqttProtocolVersion::V5 => self.options_v5.as_ref().map_or_else(String::new, |o| {
                let (host, port) = o.broker_address();
                format!("{host}:{port}")
            }),
        }
    }

    /// TODO: Right now there is no way to implement the healthcheck properly: <https://github.com/bytebeamio/rumqtt/issues/562>
    pub async fn healthcheck(&self) -> crate::Result<()> {
        Ok(())
    }
}

/// Builds an MqttConnector from the common config and additional parameters.
pub fn build_connector(
    common: &MqttCommonConfig,
    client_id_prefix: &str,
    clean_session: bool,
    manual_acks: bool,
) -> Result<MqttConnector, MqttError> {
    let client_id = common.client_id.clone().unwrap_or_else(|| {
        let hash = rand::rng()
            .sample_iter(&rand_distr::Alphanumeric)
            .take(6)
            .map(char::from)
            .collect::<String>();
        format!("{client_id_prefix}{hash}")
    });

    if client_id.is_empty() {
        return Err(ConfigurationError::EmptyClientId).context(ConfigurationSnafu);
    }

    let tls = MaybeTlsSettings::from_config(common.tls.as_ref(), false).context(TlsSnafu)?;

    match (&common.user, &common.password) {
        (Some(_), Some(_)) | (None, None) => {}
        _ => {
            return Err(ConfigurationError::IncompleteCredentials).context(ConfigurationSnafu);
        }
    }

    match common.protocol_version {
        MqttProtocolVersion::V311 => {
            let mut options = MqttOptionsV3::new(&client_id, &common.host, common.port);
            options.set_keep_alive(Duration::from_secs(common.keep_alive.into()));
            options.set_max_packet_size(common.max_packet_size, common.max_packet_size);
            options.set_clean_session(clean_session);
            options.set_manual_acks(manual_acks);

            if let (Some(user), Some(password)) = (&common.user, &common.password) {
                options.set_credentials(user, password);
            }

            if let Some(tls) = tls.tls() {
                let ca = tls.authorities_pem().flatten().collect();
                let client_auth = tls.identity_pem();
                let alpn = Some(vec!["mqtt".into()]);
                options.set_transport(Transport::Tls(TlsConfiguration::Simple {
                    ca,
                    client_auth,
                    alpn,
                }));
            }

            Ok(MqttConnector {
                protocol_version: MqttProtocolVersion::V311,
                options_v3: Some(options),
                options_v5: None,
            })
        }
        MqttProtocolVersion::V5 => {
            let mut options = MqttOptionsV5::new(&client_id, &common.host, common.port);
            options.set_keep_alive(Duration::from_secs(common.keep_alive.into()));
            options.set_clean_start(clean_session);
            options.set_manual_acks(manual_acks);

            if let (Some(user), Some(password)) = (&common.user, &common.password) {
                options.set_credentials(user, password);
            }

            // Set v5-specific connect properties
            if let Some(connect_properties) = &common.connect_properties {
                use rumqttc::v5::mqttbytes::v5::ConnectProperties;

                let mut props = ConnectProperties::new();

                if let Some(session_expiry) = connect_properties.session_expiry_interval {
                    props.session_expiry_interval = Some(session_expiry);
                }

                if let Some(topic_alias_max) = connect_properties.topic_alias_max {
                    props.topic_alias_max = Some(topic_alias_max);
                }

                if !connect_properties.user_properties.is_empty() {
                    props.user_properties = connect_properties
                        .user_properties
                        .iter()
                        .map(|property| (property.key.clone(), property.value.clone()))
                        .collect();
                }

                props.max_packet_size = Some(common.max_packet_size as u32);

                options.set_connect_properties(props);
            } else {
                // Set max packet size even without explicit v5 connect config
                use rumqttc::v5::mqttbytes::v5::ConnectProperties;
                let mut props = ConnectProperties::new();
                props.max_packet_size = Some(common.max_packet_size as u32);
                options.set_connect_properties(props);
            }

            if let Some(tls) = tls.tls() {
                let ca = tls.authorities_pem().flatten().collect();
                let client_auth = tls.identity_pem();
                let alpn = Some(vec!["mqtt".into()]);
                options.set_transport(Transport::Tls(TlsConfiguration::Simple {
                    ca,
                    client_auth,
                    alpn,
                }));
            }

            Ok(MqttConnector {
                protocol_version: MqttProtocolVersion::V5,
                options_v3: None,
                options_v5: Some(options),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigurationError, MqttPublishProperties, MqttUserProperty};

    #[test]
    fn publish_properties_preserve_binary_and_duplicate_user_properties() {
        let properties = MqttPublishProperties {
            correlation_data: Some(vec![0x66, 0x6f, 0x80, 0xff]),
            user_properties: vec![
                MqttUserProperty {
                    key: "x-test".to_string(),
                    value: "first".to_string(),
                },
                MqttUserProperty {
                    key: "x-test".to_string(),
                    value: "second".to_string(),
                },
            ],
            ..Default::default()
        };

        let actual = properties.to_publish_properties().unwrap();

        assert_eq!(
            actual.correlation_data,
            Some(bytes::Bytes::from_static(&[0x66, 0x6f, 0x80, 0xff]))
        );
        assert_eq!(
            actual.user_properties,
            vec![
                ("x-test".to_string(), "first".to_string()),
                ("x-test".to_string(), "second".to_string()),
            ]
        );
    }

    #[test]
    fn publish_properties_reject_invalid_payload_format_indicator() {
        let properties = MqttPublishProperties {
            payload_format_indicator: Some(2),
            ..Default::default()
        };

        assert_eq!(
            properties.to_publish_properties(),
            Err(ConfigurationError::InvalidPayloadFormatIndicator { value: 2 })
        );
    }
}
