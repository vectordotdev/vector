mod config;
mod sink;

use crate::config::SinkDescription;
pub use config::{ConsoleSinkConfig, Target};

inventory::submit! {
    SinkDescription::new::<ConsoleSinkConfig>("console")
}
