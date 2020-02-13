#![allow(clippy::new_without_default, clippy::needless_pass_by_value)]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate derivative;

#[cfg(feature = "jemallocator")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

pub mod buffers;
pub mod conditions;
pub mod config_paths;
pub mod dns;
pub mod event;
pub mod generate;
pub mod list;
pub mod metrics;
pub mod region;
pub mod runtime;
pub mod sinks;
pub mod sources;
pub mod template;
pub mod test_util;
pub mod topology;
pub mod trace;
pub mod transforms;
pub mod types;
pub mod unit_test;

pub use event::Event;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type Result<T> = std::result::Result<T, Error>;
