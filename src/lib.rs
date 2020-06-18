#![allow(clippy::approx_constant)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::blacklisted_name)]
#![allow(clippy::block_in_if_condition_stmt)]
#![allow(clippy::clone_double_ref)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::double_must_use)]
#![allow(clippy::drop_ref)]
#![allow(clippy::expect_fun_call)]
#![allow(clippy::filter_next)]
#![allow(clippy::float_cmp)]
#![allow(clippy::identity_conversion)]
#![allow(clippy::identity_op)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::inefficient_to_string)]
#![allow(clippy::into_iter_on_ref)]
#![allow(clippy::iter_nth_zero)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::len_zero)]
#![allow(clippy::let_and_return)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::map_clone)]
#![allow(clippy::match_bool)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::match_wild_err_arm)]
#![allow(clippy::needless_bool)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::needless_return)]
#![allow(clippy::needless_update)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::new_without_default)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::option_as_ref_deref)]
#![allow(clippy::option_map_unit_fn)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::redundant_static_lifetimes)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::single_component_path_imports)]
#![allow(clippy::single_match)]
#![allow(clippy::string_lit_as_bytes)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::toplevel_ref_arg)]
#![allow(clippy::trivial_regex)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::try_err)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unit_arg)]
#![allow(clippy::unnecessary_unwrap)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::unused_unit)]
#![allow(clippy::useless_format)]
#![allow(clippy::wrong_self_convention)]
#![allow(clippy::zero_prefixed_literal)]

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
