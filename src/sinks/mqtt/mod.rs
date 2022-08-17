mod config;
mod sink;

pub use config::MqttSinkConfig;

use crate::config::SinkDescription;

const NAME: &str = "mqtt";

inventory::submit! {
    SinkDescription::new::<MqttSinkConfig>(NAME)
}
