//! `webhdfs` sink.
//!
//! This sink will send it's output to WEBHDFS.
//!
//! `webhdfs` is an OpenDal based services. This mod itself only provide
//! config to build an [`OpenDalSink`]. All real implement are powered by
//! [`OpenDalSink`].

mod config;
pub use self::config::WebHdfsConfig;

#[cfg(test)]
mod test;

#[cfg(all(test, feature = "webhdfs-integration-tests"))]
mod integration_tests;
