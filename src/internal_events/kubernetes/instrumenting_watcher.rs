// ## skip check-events ##

use std::fmt::Debug;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct WatchRequestInvoked;

impl InternalEvent for WatchRequestInvoked {
    fn emit_metrics(&self) {
        counter!("k8s_watch_requests_invoked_total", 1);
    }
}

#[derive(Debug)]
pub struct WatchRequestInvocationFailed<E> {
    pub error: E,
}

impl<E: Debug> InternalEvent for WatchRequestInvocationFailed<E> {
    fn emit_logs(&self) {
        error!(message = "Watch invocation failed.", error = ?self.error, internal_log_rate_secs = 5);
    }

    fn emit_metrics(&self) {
        counter!("k8s_watch_requests_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct WatchStreamFailed<E> {
    pub error: E,
}

impl<E: Debug> InternalEvent for WatchStreamFailed<E> {
    fn emit_logs(&self) {
        error!(message = "Watch stream failed.", error = ?self.error, internal_log_rate_secs = 5);
    }

    fn emit_metrics(&self) {
        counter!("k8s_watch_stream_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct WatchStreamItemObtained;

impl InternalEvent for WatchStreamItemObtained {
    fn emit_metrics(&self) {
        counter!("k8s_watch_stream_items_obtained_total", 1);
    }
}
