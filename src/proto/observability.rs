#![allow(clippy::clone_on_ref_ptr)]
#![allow(warnings, clippy::pedantic, clippy::nursery)]

tonic::include_proto!("vector.observability.v1");

pub use observability_service_client::ObservabilityServiceClient as Client;
pub use observability_service_server::{
    ObservabilityService as Service, ObservabilityServiceServer as Server,
};

/// File descriptor set for gRPC reflection
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/protobuf-fds.bin"));
