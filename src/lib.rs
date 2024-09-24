#![recursion_limit = "256"] // for async-stream
#![deny(unreachable_pub)]
#![deny(unused_extern_crates)]
#![deny(unused_allocation)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
#![deny(warnings)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]
#![allow(async_fn_in_trait)]
#![allow(clippy::approx_constant)]
#![allow(clippy::float_cmp)]
#![allow(clippy::match_wild_err_arm)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unit_arg)]
#![deny(clippy::clone_on_ref_ptr)]
#![deny(clippy::trivially_copy_pass_by_ref)]
#![deny(clippy::disallowed_methods)] // [nursery] mark some functions as verboten
#![deny(clippy::missing_const_for_fn)] // [nursery] valuable to the optimizer, but may produce false positives

//! The main library to support building Vector.

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate derivative;
#[macro_use]
extern crate vector_lib;

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
pub mod common;
mod convert_config;
pub mod encoding_transcode;
pub mod enrichment_tables;
pub mod extra_context;
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
pub mod net;
#[allow(unreachable_pub)]
pub(crate) mod proto;
pub mod providers;
pub mod secrets;
pub mod serde;
#[cfg(windows)]
pub mod service;
pub mod signal;
#[cfg(all(any(feature = "sinks-socket", feature = "sinks-statsd"), unix))]
pub(crate) mod sink_ext;
#[allow(unreachable_pub)]
pub mod sinks;
pub mod source_sender;
#[allow(unreachable_pub)]
pub mod sources;
pub mod stats;
#[cfg(feature = "api-client")]
#[allow(unreachable_pub)]
pub mod tap;
pub mod template;
pub mod test_util;
#[cfg(feature = "api-client")]
#[allow(unreachable_pub)]
pub mod top;
#[allow(unreachable_pub)]
pub mod topology;
pub mod trace;
#[allow(unreachable_pub)]
pub mod transforms;
pub mod types;
pub mod unit_test;
pub(crate) mod utilization;
pub mod validate;
#[cfg(windows)]
pub mod vector_windows;
pub mod vrl_cache;

pub use source_sender::SourceSender;
pub use vector_lib::{event, metrics, schema, tcp, tls};
pub use vector_lib::{shutdown, Error, Result};

static APP_NAME_SLUG: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The name used to identify this Vector application.
///
/// This can be set at compile-time through the VECTOR_APP_NAME env variable.
/// Defaults to "Vector".
pub fn get_app_name() -> &'static str {
    option_env!("VECTOR_APP_NAME").unwrap_or("Vector")
}

/// Returns a slugified version of the name used to identify this Vector application.
///
/// Defaults to "vector".
pub fn get_slugified_app_name() -> String {
    APP_NAME_SLUG
        .get_or_init(|| get_app_name().to_lowercase().replace(' ', "-"))
        .clone()
}

/// The current version of Vector in simplified format.
/// `<version-number>-nightly`.
pub fn vector_version() -> impl std::fmt::Display {
    #[cfg(feature = "nightly")]
    let pkg_version = format!("{}-nightly", built_info::PKG_VERSION);

    #[cfg(not(feature = "nightly"))]
    let pkg_version = match built_info::DEBUG {
        // If any debug info is included, consider it a non-release build.
        "1" | "2" | "true" => {
            format!(
                "{}-custom-{}",
                built_info::PKG_VERSION,
                built_info::GIT_SHORT_HASH
            )
        }
        _ => built_info::PKG_VERSION.to_string(),
    };

    pkg_version
}

/// Returns a string containing full version information of the current build.
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

/// Includes information about the current build.
#[allow(warnings)]
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// Returns the host name of the current system.
pub fn get_hostname() -> std::io::Result<String> {
    Ok(hostname::get()?.to_string_lossy().into())
}

/// Spawn a task with the given name. The name is only used if
/// built with [`tokio_unstable`][tokio_unstable].
///
/// [tokio_unstable]: https://docs.rs/tokio/latest/tokio/#unstable-features
#[track_caller]
pub(crate) fn spawn_named<T>(
    task: impl std::future::Future<Output = T> + Send + 'static,
    _name: &str,
) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
{
    #[cfg(tokio_unstable)]
    return tokio::task::Builder::new()
        .name(_name)
        .spawn(task)
        .expect("tokio task should spawn");

    #[cfg(not(tokio_unstable))]
    tokio::spawn(task)
}

/// Returns an estimate of the number of recommended threads that Vector should spawn.
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
