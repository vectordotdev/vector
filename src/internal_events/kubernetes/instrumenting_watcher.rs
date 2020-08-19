use super::InternalEvent;
use metrics::counter;
use std::fmt::Debug;

#[derive(Debug)]
pub struct WatchRequestInvoked;

impl InternalEvent for WatchRequestInvoked {
    fn emit_metrics(&self) {
        counter!("k8s_watch_requests_invoked", 1);
    }
}

#[derive(Debug)]
pub struct WatchRequestInvocationFailed<E> {
    pub error: E,
}

impl<E: Debug> InternalEvent for WatchRequestInvocationFailed<E> {
    fn emit_logs(&self) {
        error!(message = "Watch invocation failed", error = ?self.error, rate_limit_secs = 5);
    }

    fn emit_metrics(&self) {
        counter!("k8s_watch_requests_failed", 1);
    }
}

#[derive(Debug)]
pub struct WatchStreamItemObtained;

impl InternalEvent for WatchStreamItemObtained {
    fn emit_metrics(&self) {
        counter!("k8s_watch_stream_items_obtained", 1);
    }
}

#[derive(Debug)]
pub struct WatchStreamErrored<E> {
    pub error: E,
}

impl<E: Debug> InternalEvent for WatchStreamErrored<E> {
    fn emit_logs(&self) {
        error!(message = "watch stream errored", error = ?self.error, rate_limit_secs = 5);
    }

    fn emit_metrics(&self) {
        counter!("k8s_watch_stream_errors", 1);
    }
}
