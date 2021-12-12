use crate::config::SinkDescription;

mod config;
mod model;
mod encoding;
mod sink;
mod service;
mod healthcheck;

pub use config::*;
pub use model::*;
pub use encoding::*;
pub use sink::*;
pub use service::*;
pub use healthcheck::*;

pub use super::{
    VectorSink, Healthcheck
};

#[cfg(test)]
pub mod tests;

inventory::submit! {
    SinkDescription::new::<NewRelicConfig>("new_relic")
}