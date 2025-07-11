#![allow(missing_docs)]
mod handler;
mod schema;
mod server;
#[cfg(all(
    test,
    feature = "vector-api-tests",
    feature = "sinks-blackhole",
    feature = "sources-demo_logs",
    feature = "transforms-log_to_metric",
    feature = "transforms-remap",
))]
mod tests;

pub use schema::build_schema;
pub use server::Server;
