mod config;
mod sink;

#[cfg(all(test, feature = "mqtt-integration-tests"))]
mod integration_tests;

pub use config::MqttSinkConfig;
