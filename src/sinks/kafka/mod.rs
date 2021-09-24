use crate::config::SinkDescription;

pub(crate) mod config;
pub(crate) mod sink;

// #[cfg(test)]
// mod tests;

use self::config::KafkaSinkConfig;

inventory::submit! {
    SinkDescription::new::<KafkaSinkConfig>("kafka")
}
