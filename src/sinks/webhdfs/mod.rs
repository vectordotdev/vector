//! `webhdfs` sink.
//!
//! The Hadoop Distributed File System (HDFS) is a distributed file system
//! designed to run on commodity hardware. HDFS consists of a namenode and a
//! datanode. We will send rpc to namenode to know which datanode to send
//! and receive data to. Also, HDFS will rebalance data across the cluster
//! to make sure each file has enough redundancy.
//!
//! ```txt
//!                     ┌───────────────┐
//!                     │  Data Node 2  │
//!                     └───────────────┘
//!                             ▲
//! ┌───────────────┐           │            ┌───────────────┐
//! │  Data Node 1  │◄──────────┼───────────►│  Data Node 3  │
//! └───────────────┘           │            └───────────────┘
//!                     ┌───────┴───────┐
//!                     │   Name Node   │
//!                     └───────────────┘
//!                             ▲
//!                             │
//!                      ┌──────┴─────┐
//!                      │   Vector   │
//!                      └────────────┘
//! ```
//!
//! WebHDFS will connect to the HTTP RESTful API of HDFS.
//!
//! For more information, please refer to:
//!
//! - [HDFS Users Guide](https://hadoop.apache.org/docs/stable/hadoop-project-dist/hadoop-hdfs/HdfsUserGuide.html)
//! - [WebHDFS REST API](https://hadoop.apache.org/docs/stable/hadoop-project-dist/hadoop-hdfs/WebHDFS.html)
//! - [opendal::services::webhdfs](https://docs.rs/opendal/latest/opendal/services/struct.Webhdfs.html)
//!
//! `webhdfs` is an OpenDal based services. This mod itself only provide
//! config to build an [`crate::sinks::opendal_common::OpenDalSink`]. All real implement are powered by
//! [`crate::sinks::opendal_common::OpenDalSink`].

mod config;
pub use self::config::WebHdfsConfig;

#[cfg(test)]
mod test;

#[cfg(all(test, feature = "webhdfs-integration-tests"))]
mod integration_tests;
