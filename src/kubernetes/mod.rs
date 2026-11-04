#![allow(missing_docs)]
//! This module contains helpers Kubernetes helpers as well as a
//! `custom_reflector` which delays the removal of metadata allowing
//! us to enrich events even after the resource is deleted from the
//! Kubernetes cluster.
//!

#![cfg(feature = "kubernetes")]

pub mod meta_cache;
pub mod pod_manager_logic;
pub mod reflector;

pub use reflector::custom_reflector;
