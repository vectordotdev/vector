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
#![deny(clippy::disallowed_method)] // [nursery] mark some functions as verboten
#![deny(clippy::missing_const_for_fn)] // [nursery] valuable to the optimizer,
                                       // but may produce false positives

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate derivative;
extern crate vector_core;
#[cfg(feature = "vrl-cli")]
extern crate vrl_cli;

#[cfg(feature = "tikv-jemallocator")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[macro_use]
pub mod config;
pub mod cli;
pub mod conditions;
pub mod dns;
#[cfg(feature = "docker")]
pub mod docker;
pub mod expiring_hash_map;
pub mod generate;
#[macro_use]
pub mod internal_events;
#[cfg(feature = "api")]
pub mod api;
pub mod app;
pub mod async_read;
#[cfg(any(feature = "rusoto_core", feature = "aws-config"))]
pub mod aws;
#[cfg(feature = "codecs")]
pub mod codecs;
pub(crate) mod common;
pub mod encoding_transcode;
pub mod enrichment_tables;
pub mod graph;
pub mod heartbeat;
pub mod http;
#[cfg(any(feature = "sources-kafka", feature = "sinks-kafka"))]
pub(crate) mod kafka;
pub mod kubernetes;
pub mod line_agg;
pub mod list;
pub(crate) mod proto;
pub mod providers;
pub mod serde;
#[cfg(windows)]
pub mod service;
pub mod shutdown;
pub mod signal;
pub mod sink;
pub mod sinks;
pub mod source_sender;
pub mod sources;
pub mod stats;
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

pub use source_sender::SourceSender;
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
    let build_desc = built_info::VECTOR_BUILD_DESC;
    let build_string = match build_desc {
        Some(desc) => format!("{} {}", built_info::TARGET, desc),
        None => built_info::TARGET.into(),
    };

    // We do not add 'debug' to the BUILD_DESC unless the caller has flagged on line
    // or full debug symbols. See the Cargo Book profiling section for value meaning:
    // https://doc.rust-lang.org/cargo/reference/profiles.html#debug
    let build_string = match built_info::DEBUG {
        "1" => format!("{} debug=line", build_string),
        "2" | "true" => format!("{} debug=full", build_string),
        _ => build_string,
    };

    format!("{} ({})", pkg_version, build_string)
}

#[allow(unused)]
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub fn get_hostname() -> std::io::Result<String> {
    Ok(hostname::get()?.to_string_lossy().into())
}
