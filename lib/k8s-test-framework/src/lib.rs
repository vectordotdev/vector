#![deny(warnings)]

//! Kubernetes test framework.
//!
//! The main goal of the design of this test framework is to wire kubernetes
//! components testing through the same tools that are available to the
//! developer as executable commands, rather than using a rust interface to talk
//! to k8s cluster directly.
//! This enables very trivial troubleshooting and allows us to use the same
//! deployment mechanisms that we use for production - effectively giving us
//! the opportunity to test e2e: not just the code layer, but also the
//! deployment configuration.

#![deny(
    missing_debug_implementations,
    missing_copy_implementations,
    missing_docs
)]

mod exec_tail;
pub mod framework;
mod helm_values_file;
pub mod interface;
pub mod kubernetes_version;
mod lock;
mod log_lookup;
pub mod namespace;
mod pod;
mod port_forward;
mod reader;
mod resource_file;
pub mod restart_rollout;
mod temp_file;
pub mod test_pod;
mod up_down;
mod util;
pub mod vector;
pub mod wait_for_resource;
pub mod wait_for_rollout;

// Re-export some unit for trivial accessibility.

use exec_tail::exec_tail;
pub use framework::Framework;
pub use interface::Interface;
pub use lock::lock;
use log_lookup::log_lookup;
use port_forward::port_forward;
pub use port_forward::PortForwarder;
pub use reader::Reader;
pub use test_pod::CommandBuilder;
pub use up_down::Manager;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
