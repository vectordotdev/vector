#![allow(clippy::clone_on_ref_ptr)]
#![allow(warnings, clippy::pedantic, clippy::nursery)]

tonic::include_proto!("vector.observability");

pub use observability_client::ObservabilityClient as Client;
pub use observability_server::{Observability as Service, ObservabilityServer as Server};
