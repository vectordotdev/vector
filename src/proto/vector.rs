#![allow(clippy::clone_on_ref_ptr)]

tonic::include_proto!("vector");

pub(crate) use vector_client::VectorClient as Client;
pub(crate) use vector_server::{Vector as Service, VectorServer as Server};
