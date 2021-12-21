mod config;
mod sink;

pub use config::{ConsoleSinkConfig, Target};

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<ConsoleSinkConfig>("console")
}
