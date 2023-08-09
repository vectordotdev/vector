//! This module contains a generic implementation of `Service` and related infrastructure
//! for HTTP based stream sinks.
//!
//! In particular, HTTP based stream sinks can use the `HttpService` and only need to define
//! a struct that implements the `HttpServiceRequestBuilder` trait.
//!
//! The `HttpRequest` is used in the `RequestBuilder` implementation.

mod request;
mod retry;
mod service;

pub use request::HttpRequest;
pub use retry::HttpRetryLogic;
pub use service::{HttpService, HttpServiceRequestBuilder};
