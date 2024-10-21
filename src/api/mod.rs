#![allow(missing_docs)]
mod handler;
mod schema;
mod server;
#[cfg(all(test, feature = "vector-api-tests"))]
mod tests;

pub use schema::build_schema;
pub use server::Server;
