use crate::config::SinkDescription;

mod config;
mod sink;

#[cfg(test)]
mod tests;

pub use self::config::S3SinkConfig;

inventory::submit! {
    SinkDescription::new::<S3SinkConfig>("aws_s3")
}
