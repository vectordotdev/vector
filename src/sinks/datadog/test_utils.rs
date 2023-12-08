#![cfg(any(feature = "sinks-datadog_logs", feature = "sinks-datadog_metrics"))]
use bytes::Bytes;
use http::status::StatusCode;

use crate::sinks::util::test::build_test_server_status;

// The sink must support v1 and v2 API endpoints which have different codes for
// signaling status. This enum allows us to signal which API endpoint and what
// kind of response we want our test to model without getting into the details
// of exactly what that code is.
pub(super) enum ApiStatus {
    OKv1,
    #[cfg(feature = "sinks-datadog_logs")]
    OKv2,
    #[cfg(feature = "sinks-datadog_logs")]
    BadRequestv1,
    #[cfg(feature = "sinks-datadog_logs")]
    BadRequestv2,
}

pub(super) fn test_server(
    addr: std::net::SocketAddr,
    api_status: ApiStatus,
) -> (
    futures::channel::mpsc::Receiver<(http::request::Parts, Bytes)>,
    stream_cancel::Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    let status = match api_status {
        ApiStatus::OKv1 => StatusCode::OK,
        #[cfg(feature = "sinks-datadog_logs")]
        ApiStatus::OKv2 => StatusCode::ACCEPTED,
        #[cfg(feature = "sinks-datadog_logs")]
        ApiStatus::BadRequestv1 | ApiStatus::BadRequestv2 => StatusCode::BAD_REQUEST,
    };

    // NOTE: we pass `Trigger` out to the caller even though this suite never
    // uses it as it being dropped cancels the stream machinery here,
    // indicating that failures that might not be valid.
    build_test_server_status(addr, status)
}
