//! Trait and related types for Kubernetes HTTP Responses.

use k8s_openapi::Response as K8sResponse;
use k8s_openapi::{
    apimachinery::pkg::apis::meta::v1::WatchEvent, Resource, ResponseError, WatchResponse,
};
use serde::de::DeserializeOwned;

/// `Response` is used for deserializing Kubernetes API Responses.
/// It differs from [`DeserializeOwned`] in that it is only implemented
/// for types that can be deserialized as a _whole_ response object
/// (it does not support partial or incomplete objects).
pub trait Response: Sized {
    /// Tries to parse the response from the response body buffer.
    fn from_buf(buf: &[u8]) -> Result<(Self, usize), Error>;
}

impl<T> Response for WatchEvent<T>
where
    T: DeserializeOwned + Resource,
{
    fn from_buf(buf: &[u8]) -> Result<(Self, usize), Error> {
        K8sResponse::try_from_parts(http::StatusCode::OK, buf)
            .map(|item| match item {
                (WatchResponse::Ok(val), bytes) => (val, bytes),
                (WatchResponse::Other(_), _) => unreachable!(
                    "We explicitly hardcode StatusCode::Ok so this should never get called."
                ),
            })
            .map_err(Into::into)
    }
}

/// The type of errors from parsing an HTTP response as one of the Kubernetes API functions' response types.
#[derive(Debug)]
pub enum Error {
    /// An error from deserializing the HTTP response, indicating more data is needed to complete deserialization.
    NeedMoreData,

    /// An error while deserializing the HTTP response as a JSON value, indicating the response is malformed.
    Json(serde_json::Error),

    /// An error while deserializing the HTTP response as a string, indicating that the response data is not UTF-8.
    Utf8(std::str::Utf8Error),
}

impl From<ResponseError> for Error {
    fn from(error: ResponseError) -> Self {
        match error {
            ResponseError::NeedMoreData => Error::NeedMoreData,
            ResponseError::Utf8(err) => Error::Utf8(err),
            ResponseError::Json(err) => Error::Json(err),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NeedMoreData => write!(f, "need more response data"),
            Error::Json(err) => write!(f, "{}", err),
            Error::Utf8(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::NeedMoreData => None,
            Error::Json(err) => Some(err),
            Error::Utf8(err) => Some(err),
        }
    }
}
