#![deny(warnings)]
#![deny(clippy::all)]

pub mod file_server;
pub mod file_watcher;
pub mod paths_provider;
pub mod scheduler;
#[cfg(test)]
mod sim;
