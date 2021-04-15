pub mod http;

use crate::{config::Config, control::Control};
use tokio::sync::mpsc;

pub type ProviderTx = mpsc::Sender<ProviderControl>;
pub type ProviderRx = mpsc::Receiver<ProviderControl>;

pub enum ProviderControl {
    Config(Config),
}

impl From<ProviderControl> for Control {
    fn from(provider_control: ProviderControl) -> Self {
        match provider_control {
            ProviderControl::Config(config) => Control::Config(config),
        }
    }
}

fn provider_control() -> (ProviderTx, ProviderRx) {
    mpsc::channel(10)
}
