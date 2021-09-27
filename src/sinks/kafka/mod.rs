use crate::config::SinkDescription;

pub(crate) mod config;
pub(crate) mod sink;
pub(crate) mod service;
pub(crate) mod request_builder;

// #[cfg(test)]
// mod tests;

use self::config::KafkaSinkConfig;

inventory::submit! {
    SinkDescription::new::<KafkaSinkConfig>("kafka")
}
