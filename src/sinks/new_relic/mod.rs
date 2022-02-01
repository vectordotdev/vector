use crate::config::SinkDescription;

mod config;
mod encoding;
mod healthcheck;
mod model;
mod service;
mod sink;

pub use config::*;
pub use encoding::*;
pub use healthcheck::*;
pub use model::*;
pub use service::*;
pub use sink::*;

pub use super::{Healthcheck, VectorSink};

#[cfg(test)]
pub mod tests;

inventory::submit! {
    SinkDescription::new::<NewRelicConfig>("new_relic")
}
