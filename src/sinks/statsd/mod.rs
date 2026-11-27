mod batch;
mod config;
mod encoder;
mod normalizer;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;

pub use self::config::StatsdSinkConfig;
