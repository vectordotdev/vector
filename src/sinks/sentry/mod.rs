//! The Sentry [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to Sentry via HTTP.
mod config;
mod constants;
mod encoder;
mod log_convert;
mod request_builder;
mod service;
mod sink;

pub use self::config::SentryConfig;

#[cfg(feature = "sentry-integration-tests")]
mod integration_tests;
