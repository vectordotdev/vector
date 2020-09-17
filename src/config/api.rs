use super::ConfigBuilder;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Default, Debug, Deserialize, Serialize, PartialEq, Copy, Clone)]
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
pub fn update_config(old_config: &mut ConfigBuilder, new_config: &ConfigBuilder) {
    old_config.api = new_config.api; // copy
}
