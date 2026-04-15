//! Vector gRPC API client library
//!
//! This library provides a Rust client for the Vector gRPC observability API.
//!
//! # Example
//!
//! ```no_run
//! use vector_api_client::Client;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = Client::new("http://localhost:9999".parse().unwrap());
//! client.connect().await?;
//!
//! // Check health (standard gRPC health check)
//! client.health().await?;
//! println!("Server is healthy");
//!
//! // Get components
//! let components = client.get_components(0).await?;
//! for component in components.components {
//!     println!("Component: {}", component.component_id);
//! }
//! # Ok(())
//! # }
//! ```

mod client;
mod error;

pub use client::Client;
pub use error::{Error, Result};

/// How long (ms) to wait before attempting to reconnect to the Vector API after a disconnect.
pub const RECONNECT_DELAY_MS: u64 = 5000;

/// Re-export generated protobuf types
pub mod proto {
    pub mod event {
        tonic::include_proto!("event");
    }

    tonic::include_proto!("vector.observability.v1");
}
