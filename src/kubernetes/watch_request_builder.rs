//! Build watch request for k8s API and adapters for library types.
//!
//! # Examples
//!
//! ## Non-namespaced and cluster-global
//!
//! ```rust
//! # use vector::kubernetes::watch_request_builder::WatchRequestBuilder;
//! use k8s_openapi::api::core::v1::Pod;
//!
//! let _val: Box<dyn WatchRequestBuilder<Object = Pod>> =
//!   Box::new(Pod::watch_pod_for_all_namespaces);
//! ```
//!
//! ## Namespaced
//!
//! ```rust
//! # use vector::kubernetes::watch_request_builder::{WatchRequestBuilder, Namespaced};
//! use k8s_openapi::api::core::v1::Pod;
//!
//! let _val: Box<dyn WatchRequestBuilder<Object = Pod>> =
//!   Box::new(Namespaced("default", Pod::watch_namespaced_pod));
//! ```
//!

use k8s_openapi::{
    http::{Request, StatusCode},
    RequestError, Resource, ResponseBody, WatchOptional, WatchResponse,
};
use serde::de::DeserializeOwned;

/// Build a watch request for the k8s API.
///
/// See module documentation.
pub trait WatchRequestBuilder {
    /// The object type that's being watched.
    type Object: Resource + DeserializeOwned;

    /// Build a watch request.
    fn build<'a>(
        &self,
        watch_optional: WatchOptional<'a>,
    ) -> Result<Request<Vec<u8>>, RequestError>;
}

impl<F, T> WatchRequestBuilder for F
where
    T: Resource + DeserializeOwned,
    F: for<'w> Fn(
        WatchOptional<'w>,
    ) -> Result<
        (
            Request<Vec<u8>>,
            fn(StatusCode) -> ResponseBody<WatchResponse<T>>,
        ),
        RequestError,
    >,
{
    type Object = T;

    fn build<'a>(
        &self,
        watch_optional: WatchOptional<'a>,
    ) -> Result<Request<Vec<u8>>, RequestError> {
        let (request, _) = (self)(watch_optional)?;
        Ok(request)
    }
}

/// Wrapper for a namespaced API.
///
/// Specify the namespace and an API request building function.
///
/// See module documentation for an example.
pub struct Namespaced<N, F>(pub N, pub F);

impl<N, F, T> WatchRequestBuilder for Namespaced<N, F>
where
    N: AsRef<str>,
    T: Resource + DeserializeOwned,
    F: for<'w> Fn(
        &'w str,
        WatchOptional<'w>,
    ) -> Result<
        (
            Request<Vec<u8>>,
            fn(StatusCode) -> ResponseBody<WatchResponse<T>>,
        ),
        RequestError,
    >,
{
    type Object = T;

    fn build<'a>(
        &self,
        watch_optional: WatchOptional<'a>,
    ) -> Result<Request<Vec<u8>>, RequestError> {
        let (request, _) = (self.1)(self.0.as_ref(), watch_optional)?;
        Ok(request)
    }
}
