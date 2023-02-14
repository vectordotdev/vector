#![recursion_limit = "256"] // for async-stream
#![deny(unreachable_pub)]
#![deny(unused_extern_crates)]
#![deny(unused_allocation)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
#![deny(warnings)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]
#![allow(clippy::approx_constant)]
#![allow(clippy::float_cmp)]
#![allow(clippy::match_wild_err_arm)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unit_arg)]
#![deny(clippy::clone_on_ref_ptr)]
#![deny(clippy::trivially_copy_pass_by_ref)]
#![deny(clippy::disallowed_methods)] // [nursery] mark some functions as verboten
#![deny(clippy::missing_const_for_fn)] // [nursery] valuable to the optimizer,
                                       // but may produce false positives

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate derivative;

#[cfg(all(feature = "tikv-jemallocator", not(feature = "allocation-tracing")))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "tikv-jemallocator", feature = "allocation-tracing"))]
#[global_allocator]
static ALLOC: self::internal_telemetry::allocations::Allocator<tikv_jemallocator::Jemalloc> =
    self::internal_telemetry::allocations::get_grouped_tracing_allocator(
        tikv_jemallocator::Jemalloc,
    );

#[allow(unreachable_pub)]
pub mod internal_telemetry;

#[macro_use]
#[allow(unreachable_pub)]
pub mod config;
pub mod cli;
#[allow(unreachable_pub)]
pub mod components;
pub mod conditions;
#[cfg(not(windows))]
pub mod control_server;
pub mod dns;
#[cfg(feature = "docker")]
pub mod docker;
pub mod expiring_hash_map;
pub mod generate;
pub mod generate_schema;
#[macro_use]
#[allow(unreachable_pub)]
pub mod internal_events;
#[cfg(feature = "lapin")]
pub mod amqp;
#[cfg(feature = "api")]
#[allow(unreachable_pub)]
pub mod api;
pub mod app;
pub mod async_read;
#[cfg(feature = "aws-config")]
pub mod aws;
#[allow(unreachable_pub)]
pub mod codecs;
pub(crate) mod common;
pub mod encoding_transcode;
pub mod enrichment_tables;
#[cfg(feature = "gcp")]
pub mod gcp;
pub(crate) mod graph;
pub mod heartbeat;
pub mod http;
#[allow(unreachable_pub)]
#[cfg(any(feature = "sources-kafka", feature = "sinks-kafka"))]
pub mod kafka;
#[allow(unreachable_pub)]
pub mod kubernetes;
pub mod line_agg;
pub mod list;
#[cfg(any(feature = "sources-nats", feature = "sinks-nats"))]
pub(crate) mod nats;
#[allow(unreachable_pub)]
pub(crate) mod proto;
pub mod providers;
pub mod secrets;
pub mod serde;
#[cfg(windows)]
pub mod service;
pub mod signal;
pub(crate) mod sink;
#[allow(unreachable_pub)]
pub mod sinks;
pub mod source_sender;
#[allow(unreachable_pub)]
pub mod sources;
pub mod stats;
#[cfg(feature = "api-client")]
#[allow(unreachable_pub)]
mod tap;
pub mod template;
pub mod test_util;
#[cfg(feature = "api-client")]
#[allow(unreachable_pub)]
pub(crate) mod top;
#[allow(unreachable_pub)]
pub mod topology;
pub mod trace;
#[allow(unreachable_pub)]
pub mod transforms;
pub mod types;
pub mod udp;
pub mod unit_test;
pub(crate) mod utilization;
pub mod validate;
#[cfg(windows)]
pub mod vector_windows;

pub use source_sender::SourceSender;
pub use vector_common::{shutdown, Error, Result};
pub use vector_core::{event, metrics, schema, tcp, tls};

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

#[allow(warnings)]
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub fn get_hostname() -> std::io::Result<String> {
    Ok(hostname::get()?.to_string_lossy().into())
}

#[track_caller]
pub(crate) fn spawn_named<T>(
    task: impl std::future::Future<Output = T> + Send + 'static,
    _name: &str,
) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
{
    #[cfg(tokio_unstable)]
    return tokio::task::Builder::new().name(_name).spawn(task);

    #[cfg(not(tokio_unstable))]
    tokio::spawn(task)
}

pub fn num_threads() -> usize {
    let count = match std::thread::available_parallelism() {
        Ok(count) => count,
        Err(error) => {
            warn!(message = "Failed to determine available parallelism for thread count, defaulting to 1.", %error);
            std::num::NonZeroUsize::new(1).unwrap()
        }
    };
    usize::from(count)
}
