use super::Config;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Default, Debug, Deserialize, Serialize, PartialEq)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_bind")]
    pub bind: Option<SocketAddr>,

    #[serde(default = "default_playground")]
    pub playground: bool,
}

/// Determines whether the API server should start, stop or restart. Used by configuration
/// and topology to spawn/stop the API server if needed, typically on config changes.
pub enum Difference {
    Start,
    Stop,
    Restart,
}

impl Difference {
    /// Determines whether the API server should start, stop or restart, based on the
    /// previous configuration options, and the new ones
    pub fn new(old: &Options, new: &Options) -> Option<Self> {
        match (old.enabled, new.enabled) {
            (false, true) => Some(Self::Start),
            (true, false) => Some(Self::Stop),
            (true, true) if *old != *new => Some(Self::Restart),
            _ => None,
        }
    }

    /// Returns `true` if the API server should stop|restart
    pub fn is_stop_or_restart(&self) -> bool {
        match &self {
            Self::Stop | Self::Restart => true,
            _ => false,
        }
    }

    /// Returns `true` if the API server should start|restart
    pub fn is_start_or_restart(&self) -> bool {
        match &self {
            Self::Start | Self::Restart => true,
            _ => false,
        }
    }
}

/// Updates the configuration to take into account API changes
pub fn update_config(old_config: &mut Config, new_config: &Config) {
    // API enablement
    if new_config.api.enabled != default_enabled() {
        old_config.api.enabled = new_config.api.enabled
    }

    // IP/port
    if let Some(bind) = new_config.api.bind {
        old_config.api.bind = Some(bind)
    }

    // Playground
    if new_config.api.playground != default_playground() {
        old_config.api.playground = new_config.api.playground;
    }
}

fn default_enabled() -> bool {
    false
}

pub fn default_bind() -> Option<SocketAddr> {
    Some(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8686))
}

fn default_playground() -> bool {
    true
}
