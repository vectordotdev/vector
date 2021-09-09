use crate::config::SinkDescription;

pub(crate) mod config;
pub(crate) mod partitioner;
pub(crate) mod service;
pub(crate) mod sink;

#[cfg(test)]
mod tests;

use self::config::S3SinkConfig;

inventory::submit! {
    SinkDescription::new::<S3SinkConfig>("aws_s3")
}
