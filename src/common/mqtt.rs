use vector_config_macros::configurable_component;
use vector_lib::tls::TlsEnableableConfig;

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
