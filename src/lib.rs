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
#[cfg(feature = "vrl-cli")]
extern crate vrl_cli;

#[macro_use]
pub mod config;
pub mod cli;
pub mod conditions;
pub mod dns;
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
pub mod buffers;
pub mod encoding_transcode;
pub mod heartbeat;
pub mod http;
#[cfg(any(feature = "sources-kafka", feature = "sinks-kafka"))]
pub mod kafka;
pub mod kubernetes;
pub mod line_agg;
pub mod list;
pub(crate) mod pipeline;
pub(crate) mod proto;
pub mod providers;
#[cfg(feature = "rusoto_core")]
pub mod rusoto;
pub mod serde;
pub mod service;
pub mod shutdown;
pub mod signal;
pub mod sink;
pub mod sinks;
pub mod sources;
pub(crate) mod stats;
pub mod stream;
#[cfg(feature = "api-client")]
mod tap;
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
pub mod udp;
pub mod unit_test;
pub(crate) mod utilization;
pub mod validate;
#[cfg(windows)]
pub mod vector_windows;

pub use pipeline::Pipeline;

pub use vector_core::{event, mapping, metrics, Error, Result};

pub fn vector_version() -> impl std::fmt::Display {
    #[cfg(feature = "nightly")]
    let pkg_version = format!("{}-nightly", built_info::PKG_VERSION);

    #[cfg(not(feature = "nightly"))]
    let pkg_version = built_info::PKG_VERSION;

    pkg_version
}

pub fn get_version() -> String {
    let pkg_version = vector_version();
    let commit_hash = built_info::get_commit_hash();
    let built_date = built_info::get_built_date();
    let built_string = if commit_hash.is_some() && built_date.is_some() {
        format!(
            "{} {} {}",
            commit_hash.unwrap(),
            built_info::TARGET,
            built_date.unwrap()
        )
    } else {
        built_info::TARGET.into()
    };
    format!("{} ({})", pkg_version, built_string)
}

#[allow(unused)]
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));

    #[cfg(not(feature = "ci"))]
    pub fn get_built_date() -> Option<String> {
        chrono::DateTime::parse_from_rfc2822(self::BUILT_TIME_UTC)
            .ok()
            .map(|dt| dt.format("%Y-%m-%d").to_string())
    }

    #[cfg(feature = "ci")]
    pub fn get_built_date() -> Option<String> {
        None
    }

    #[cfg(not(feature = "ci"))]
    pub fn get_git_version() -> Option<String> {
        self::GIT_VERSION.map(|s| s.to_string())
    }

    #[cfg(feature = "ci")]
    pub fn get_git_version() -> Option<String> {
        None
    }

    pub fn get_commit_hash() -> Option<String> {
        get_git_version().and_then(|v| v.split('-').last().map(|s| s.to_string()))
    }
}

pub fn get_hostname() -> std::io::Result<String> {
    Ok(hostname::get()?.to_string_lossy().into())
}
