pub mod http;

use crate::config::Config;
use std::pin::Pin;
use std::task::Poll;
use std::{future::Future, task::Context};
use tokio::sync::mpsc;

pub type ProviderTx = mpsc::Sender<ProviderControl>;
pub type ProviderRx = mpsc::Receiver<ProviderControl>;

pub enum ProviderControl {
    Config(Config),
}

fn provider_control() -> (ProviderTx, ProviderRx) {
    mpsc::channel(10)
}
