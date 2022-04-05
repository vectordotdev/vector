use std::fmt::Debug;

use crate::internal_events::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct WatchRequestInvoked;

impl InternalEvent for WatchRequestInvoked {
    fn emit(self) {
        counter!("k8s_watch_requests_invoked_total", 1);
    }
}

#[derive(Debug)]
pub struct WatchRequestInvocationError<E> {
    pub error: E,
}

impl<E: Debug> InternalEvent for WatchRequestInvocationError<E> {
    fn emit(self) {
        error!(
            message = "Watch invocation failed.",
            error = ?self.error,
            internal_log_rate_secs = 5,
            error_code = "watch_request",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "watch_request",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("k8s_watch_requests_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct WatchStreamError<E> {
    pub error: E,
}

impl<E: Debug> InternalEvent for WatchStreamError<E> {
    fn emit(self) {
        error!(
            message = "Watch stream failed.",
            error = ?self.error,
            internal_log_rate_secs = 5,
            error_code = "watch_stream",
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "watch_stream",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("k8s_watch_stream_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct WatchStreamItemObtained;

impl InternalEvent for WatchStreamItemObtained {
    fn emit(self) {
        counter!("k8s_watch_stream_items_obtained_total", 1);
    }
}
