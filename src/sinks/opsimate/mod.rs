//! The OpsiMate [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to OpsiMate as custom
//! alerts. OpsiMate's custom alerts endpoint accepts exactly one alert per request, so every
//! event becomes its own request.

mod config;
mod encoder;
mod request_builder;
mod service;
mod sink;
