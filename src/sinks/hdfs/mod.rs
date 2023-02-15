//! `hdfs` sink.
//!
//! This sink will send it's output to HDFS.
//!
//! `hdfs` is an OpenDal based services. This mod itself only provide
//! config to build an [`OpenDalSink`]. All real implement are powered by
//! [`OpenDalSink`].

mod config;
pub use self::config::HdfsConfig;

#[cfg(test)]
mod test;

/// It's better to run this test inside container:
///
/// ```shell
/// make test-integration-hdfs
/// ```
///
/// However, you can still run it if you have the following setup:
///
/// - `JAVA_HOME`: install java runtime and configure `JAVA_HOME` correctly.
/// - `HADOOP_HOME`: install hadoop and configure `HADOOP_HOME` to the path of hadoop setup. Visit <https://hadoop.apache.org/releases.html> for downloading.
///
/// For more information, please refer to [opendal::services::Hdfs](https://docs.rs/opendal/latest/opendal/services/struct.Hdfs.html#environment)
#[cfg(all(test, feature = "hdfs-integration-tests"))]
mod integration_tests;
