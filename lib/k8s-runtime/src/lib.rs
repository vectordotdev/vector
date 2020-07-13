//! Runtime components of the Kubernetes API client.

#![recursion_limit = "256"] // for async-stream
#![deny(
    missing_debug_implementations,
    missing_copy_implementations,
    missing_docs
)]

#[macro_use]
extern crate tracing;

pub mod client;
pub mod debounce;
pub mod hash_value;
pub mod reflector;
pub mod resource_version;
pub mod state;
pub mod watcher;

mod test_util;

// Reexports for more elegant public API.
pub use client::Client;
pub use debounce::Debounce;
pub use hash_value::HashValue;
pub use reflector::Reflector;
