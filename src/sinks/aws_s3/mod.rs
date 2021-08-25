use crate::config::SinkDescription;

mod config;
mod partitioner;
mod service;
mod sink;

#[cfg(test)]
mod tests;

use self::config::S3SinkConfig;

inventory::submit! {
    SinkDescription::new::<S3SinkConfig>("aws_s3")
}
