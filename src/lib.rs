#![allow(clippy::approx_constant)]
#![allow(clippy::float_cmp)]
#![allow(clippy::match_wild_err_arm)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::trivial_regex)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unit_arg)]
#![allow(clippy::block_in_if_condition_stmt)]

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
pub mod expiring_hash_map;
pub mod generate;
#[cfg(feature = "wasm")]
pub mod wasm;
#[macro_use]
pub mod internal_events;
pub mod async_read;
pub mod hyper;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod list;
pub mod metrics;
pub mod region;
pub mod runtime;
pub mod serde;
pub mod shutdown;
pub mod sinks;
pub mod sources;
pub mod stream;
pub mod template;
pub mod test_util;
pub mod tls;
pub mod topology;
pub mod trace;
pub mod transforms;
pub mod types;
pub mod unit_test;
pub mod validate;

pub use event::Event;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type Result<T> = std::result::Result<T, Error>;

pub fn get_version() -> String {
    #[cfg(feature = "nightly")]
    let pkg_version = format!("{}-nightly", built_info::PKG_VERSION);
    #[cfg(not(feature = "nightly"))]
    let pkg_version = built_info::PKG_VERSION;

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
