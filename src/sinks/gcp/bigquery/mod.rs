//! The BigQuery [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance responsible for taking
//! a stream of [`vector_lib::event::Event`] and storing them in a BigQuery table.
//! This module uses the BigQuery Storage Write (gRPC) API.

#[cfg(all(test, feature = "gcp-bigquery-integration-tests"))]
mod integration_tests;

mod config;
mod request_builder;
mod service;
mod sink;

#[allow(
    warnings,
    clippy::pedantic,
    clippy::nursery,
    clippy::missing_const_for_fn,
    clippy::trivially_copy_pass_by_ref
)]
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
