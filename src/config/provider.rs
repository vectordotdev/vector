use super::ComponentDescription;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Options {}

impl Default for Options {
    fn default() -> Self {
        Self {}
    }
}

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait ProviderConfig: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn provider_type(&self) -> &'static str;
}

dyn_clone::clone_trait_object!(ProviderConfig);

pub type ProviderDescription = ComponentDescription<Box<dyn ProviderConfig>>;

inventory::collect!(ProviderDescription);
