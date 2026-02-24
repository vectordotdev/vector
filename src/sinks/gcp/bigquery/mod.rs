//! The BigQuery [`vector_core::sink::VectorSink`]
//!
//! This module contains the [`vector_core::sink::VectorSink`] instance responsible for taking
//! a stream of [`vector_core::event::Event`] and storing them in a BigQuery table.
//! This module uses the BigQuery Storage Write (gRPC) API.

#[cfg(all(test, feature = "gcp-bigquery-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

mod config;
mod request_builder;
mod service;
mod sink;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
pub(crate) mod proto {
    pub(crate) mod google {
        pub(crate) mod cloud {
            pub(crate) mod bigquery {
                pub(crate) mod storage {
                    pub(crate) mod v1 {
                        tonic::include_proto!("google.cloud.bigquery.storage.v1");
                    }
                }
            }
        }
        pub(crate) mod rpc {
            tonic::include_proto!("google.rpc");
        }
    }
}

pub use self::config::BigqueryConfig;
