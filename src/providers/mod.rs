pub mod http;

use crate::{config::Config, control::Control};
use tokio::sync::mpsc;

pub type ProviderTx = mpsc::Sender<ProviderControl>;
pub type ProviderRx = mpsc::Receiver<ProviderControl>;

/// Control messages that a provider can send. These are currently "one way" -- sent
/// from a provider, and consumed by top-level app code. Eventually, these may evolve
/// into two-way, to accommodate sending messages *back* to a provide (e.g. pub/sub.)
pub enum ProviderControl {
    Config(Config),
}

/// Top-level code isn't concerned with a provider specifically, so translate these
/// back to a top-level control message.
impl From<ProviderControl> for Control {
    fn from(provider_control: ProviderControl) -> Self {
        match provider_control {
            ProviderControl::Config(config) => Control::Config(config),
        }
    }
}

/// Return a tx/rx receiver pair for provider control messages.
fn provider_control() -> (ProviderTx, ProviderRx) {
    mpsc::channel(10)
}
