use super::Config;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Default, Debug, Deserialize, Serialize, PartialEq)]
pub struct Options {
    #[serde(default = "default_api_enabled")]
    pub enabled: bool,

    #[serde(default = "default_api_bind")]
    pub bind: Option<SocketAddr>,
}

pub enum Diff {
    Start,
    Stop,
    Restart,
}

impl Diff {
    pub fn from_api(old: &Options, new: &Options) -> Option<Self> {
        match (old.enabled, new.enabled) {
            (false, true) => Some(Diff::Start),
            (true, false) => Some(Diff::Stop),
            (true, true) if *old != *new => Some(Diff::Restart),
            _ => None,
        }
    }
}

/// Updates the configuration to take into account API changes
pub fn update_config(old_config: &mut Config, new_config: &Config) {
    if new_config.api.enabled != default_api_enabled() {
        old_config.api.enabled = new_config.api.enabled
    }

    if let Some(bind) = new_config.api.bind {
        old_config.api.bind = Some(bind)
    }
}

fn default_api_enabled() -> bool {
    false
}

fn default_api_bind() -> Option<SocketAddr> {
    Some(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8686))
}
