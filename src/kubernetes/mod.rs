//! This mod contains shared portions of the kubernetes implementations.
//!
//! Here are a few pointers to the resources that were used as an inspiration:
//!
//! - https://github.com/kubernetes/client-go/blob/master/tools/clientcmd/api/types.go
//!
//!   A part of the official Kubernetes client library (in Go) that contains
//!   the structure for KUBECONFIG files. Used for reference on naming things.
//!
//! - https://github.com/kubernetes/apimachinery/blob/master/pkg/watch/watch.go
//!
//!   The reference design of the watchers composition and interfaces that's
//!   known to work.
//!
//! - https://github.com/kubernetes/client-go/blob/master/rest/config.go
//!
//!   The reference implementation on preparing the in-cluster config.
//!

#![cfg(feature = "kubernetes")]
#![warn(missing_docs)]

pub mod any_resource;
pub mod api_watcher;
pub mod client;
pub mod debounce;
pub mod hash_value;
pub mod instrumenting_watcher;
pub mod mock_watcher;
pub mod multi_response_decoder;
pub mod reflector;
pub mod resource_version;
pub mod state;
pub mod stream;
pub mod watch_request_builder;
pub mod watcher;

// Reexports for more elegant public API.
pub use debounce::Debounce;
pub use hash_value::HashValue;
pub use multi_response_decoder::MultiResponseDecoder;
pub use reflector::Reflector;
pub use watch_request_builder::WatchRequestBuilder;
