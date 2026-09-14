#![deny(warnings)]
#![deny(clippy::all)]

pub mod file_server;
pub mod file_watcher;
pub mod notify_watcher;
pub mod paths_provider;

pub(crate) use file_source_common::absolutize;
