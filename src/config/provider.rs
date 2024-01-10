use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use vector_lib::configurable::NamedComponent;

use crate::{providers::BuildResult, signal};

/// Generalized interface for constructing a configuration from a provider.
#[async_trait]
#[enum_dispatch]
pub trait ProviderConfig: NamedComponent + core::fmt::Debug + Send + Sync {
    /// Builds a configuration.
    ///
    /// Access to signal handling is given so that the provider can control reloading and shutdown
    /// behavior as necessary.
    ///
    /// If a configuration is built successfully, `Ok(...)` is returned containing the
    /// configuration.
    ///
    /// # Errors
    ///
    /// If an error occurs while building a configuration, an error variant explaining the
    /// issue is returned.
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> BuildResult;
}
