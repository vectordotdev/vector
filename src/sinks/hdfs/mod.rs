//! `hdfs` sink.
//!
//! This sink will send it's output to HDFS.
//!
//! `hdfs` is an OpenDAL based services. This mod itself only provide
//! config to build an [`OpendalSink`]. All real implement are powered by
//! [`OpendalSink`].

mod config;
pub use self::config::HdfsConfig;

#[cfg(test)]
mod test;

#[cfg(all(test, feature = "hdfs-integration-tests"))]
mod integration_tests;
