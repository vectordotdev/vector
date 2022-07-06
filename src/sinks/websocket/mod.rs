mod config;
mod sink;

pub use config::WebSocketSinkConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<WebSocketSinkConfig>("websocket")
}
