//! Vector gRPC API client library
//!
//! This library provides a Rust client for the Vector gRPC observability API.
//! It replaces the previous GraphQL-based client with a more efficient gRPC implementation.
//!
//! # Example
//!
//! ```no_run
//! use vector_api_client::Client;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = Client::new("http://localhost:9999").await?;
//! client.connect().await?;
//!
//! // Check health
//! let health = client.health().await?;
//! println!("Healthy: {}", health.healthy);
//!
//! // Get components
//! let components = client.get_components(0).await?;
//! for component in components.components {
//!     println!("Component: {}", component.component_id);
//! }
//! # Ok(())
//! # }
//! ```

#![deny(warnings)]
#![deny(missing_debug_implementations)]

mod client;
mod error;

pub use client::GrpcClient;
// Export GrpcClient as Client for cleaner API
pub use client::GrpcClient as Client;
pub use error::{Error, Result};

/// Re-export generated protobuf types
pub mod proto {
    tonic::include_proto!("vector.observability");
}
