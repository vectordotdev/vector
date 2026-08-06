use std::collections::{HashMap, HashSet};

use enum_dispatch::enum_dispatch;
use vector_lib::configurable::NamedComponent;

use crate::signal;

/// Generalized interface to a secret backend.
#[enum_dispatch]
pub trait SecretBackend: NamedComponent + core::fmt::Debug + Send + Sync {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        signal_rx: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>>;
}
