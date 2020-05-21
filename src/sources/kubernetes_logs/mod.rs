//! This mod implements `kubernetes_logs` source.
//! The scope of this source is to consume the log files that `kubelet` keeps
//! at `/var/log/pods` at the host of the k8s node.

#![deny(missing_docs)]

mod k8s_paths_provider;
mod parser;
mod path_helpers;
