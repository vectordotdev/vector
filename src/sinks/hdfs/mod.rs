//! `hdfs` sink.
//!
//! This sink will send it's output to HDFS.
//!
//! `hdfs` is an OpenDAL based services. This mod itself only provide
//! config to build an [`OpenDALSink`]. All real implement are powered by
//! [`OpenDALSink`].

mod config;
pub use self::config::HdfsConfig;

#[cfg(test)]
mod test;

/// This test suites requires the following setups:
///
/// - `JAVA_HOME`: users must have installed java runtime and configure `JAVA_HOME` correctly.
/// - `HADOOP_HOME`: users must have installed hadoop and configure `HADOOP_HOME` to the path of hadoop setup. Visit <https://hadoop.apache.org/releases.html> for downloading.
///
/// For more information, please refer to [opendal::services::Hdfs](https://docs.rs/opendal/latest/opendal/services/struct.Hdfs.html#environment)
#[cfg(all(test, feature = "hdfs-integration-tests"))]
mod integration_tests;
