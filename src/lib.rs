#![recursion_limit = "256"] // for async-stream
#![allow(clippy::approx_constant)]
#![allow(clippy::float_cmp)]
#![allow(clippy::blocks_in_if_conditions)]
#![allow(clippy::match_wild_err_arm)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::trivial_regex)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unit_arg)]
#![deny(clippy::clone_on_ref_ptr)]
#![deny(clippy::trivially_copy_pass_by_ref)]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate derivative;
#[macro_use]
extern crate pest_derive;
#[cfg(feature = "vrl-cli")]
extern crate remap_cli;

#[cfg(feature = "jemallocator")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[macro_use]
pub mod config;
pub mod buffers;
pub mod cli;
pub mod conditions;
pub mod dns;
pub mod event;
pub mod expiring_hash_map;
pub mod generate;
#[cfg(feature = "wasm")]
pub mod wasm;
#[macro_use]
pub mod internal_events;
#[cfg(feature = "api")]
pub mod api;
pub mod app;
pub mod async_read;
pub mod encoding_transcode;
pub mod heartbeat;
pub mod http;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod kubernetes;
pub mod line_agg;
pub mod list;
pub mod mapping;
pub mod metrics;
pub(crate) mod pipeline;
#[cfg(feature = "rusoto_core")]
pub mod rusoto;
pub mod serde;
pub mod service;
pub mod shutdown;
pub mod signal;
pub mod sink;
pub mod sinks;
pub mod sources;
pub mod stream;
pub mod tcp;
pub mod template;
pub mod test_util;
pub mod tls;
#[cfg(feature = "api-client")]
pub mod top;
pub mod topology;
pub mod trace;
pub mod transforms;
pub mod trigger;
pub mod types;
#[cfg(any(feature = "sources-utils-udp", feature = "sinks-utils-udp"))]
pub mod udp;
pub mod unit_test;
pub mod validate;
#[cfg(windows)]
pub mod vector_windows;

pub use event::{Event, Value};
pub use pipeline::Pipeline;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type Result<T> = std::result::Result<T, Error>;

pub fn vector_version() -> impl std::fmt::Display {
    #[cfg(feature = "nightly")]
    let pkg_version = format!("{}-nightly", built_info::PKG_VERSION);

    #[cfg(not(feature = "nightly"))]
    let pkg_version = built_info::PKG_VERSION;

    pkg_version
}

pub fn get_version() -> String {
    let pkg_version = vector_version();
    let commit_hash = built_info::GIT_VERSION.and_then(|v| v.split('-').last());
    let built_date = chrono::DateTime::parse_from_rfc2822(built_info::BUILT_TIME_UTC)
        .unwrap()
        .format("%Y-%m-%d");
    let built_string = if let Some(commit_hash) = commit_hash {
        format!("{} {} {}", commit_hash, built_info::TARGET, built_date)
    } else {
        built_info::TARGET.into()
    };
    format!("{} ({})", pkg_version, built_string)
}

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub fn get_hostname() -> std::io::Result<String> {
    Ok(hostname::get()?.to_string_lossy().into())
}

// This is a private implementation of the unstable `bool_to_option`
// feature. This can be removed once this stabilizes:
// https://github.com/rust-lang/rust/issues/64260
trait BoolAndSome {
    fn and_some<T>(self, value: T) -> Option<T>;
}

impl BoolAndSome for bool {
    fn and_some<T>(self, value: T) -> Option<T> {
        if self {
            Some(value)
        } else {
            None
        }
    }
}
