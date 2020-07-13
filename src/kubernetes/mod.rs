//! This mod contains shared portions of the Kubernetes implementation, that's
//! Vector-specific, in a sense that they rely on the types that are defined at
//! the Vector codebase.

#![cfg(feature = "kubernetes")]
#![warn(missing_docs)]

pub mod client;
pub mod instrumenting_state;
pub mod instrumenting_watcher;

// Reexports for more elegant public API.
pub use client::Client;
