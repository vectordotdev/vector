use async_trait::async_trait;

use crate::{providers, signal};

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait ProviderConfig: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    /// Builds a provider, returning a string containing the config. It's passed a signals
    /// channel to control reloading and shutdown, as applicable.
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> providers::Result;
    fn provider_type(&self) -> &'static str;
}

dyn_clone::clone_trait_object!(ProviderConfig);
