use super::Config;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Default, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_bind")]
    pub bind: Option<SocketAddr>,

    #[serde(default = "default_playground")]
    pub playground: bool,
}

fn default_enabled() -> bool {
    false
}

fn default_bind() -> Option<SocketAddr> {
    Some(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8686))
}

fn default_playground() -> bool {
    true
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
