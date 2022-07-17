mod config;
mod sink;

pub use config::MqttSinkConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<MqttSinkConfig>("websocket")
}
