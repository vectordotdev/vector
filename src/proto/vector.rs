#![allow(clippy::clone_on_ref_ptr)]
#![allow(warnings, clippy::pedantic, clippy::nursery)]

tonic::include_proto!("vector");

pub use vector_client::VectorClient as Client;
pub use vector_server::{Vector as Service, VectorServer as Server};
