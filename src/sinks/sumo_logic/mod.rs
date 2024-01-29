//! The Sumo Logic [`vector_lib::sink::VectorSink`]
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to the Sumo Logic service.
mod config;
mod request_builder;
mod sink;
