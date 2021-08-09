use metrics::counter;
use std::error::Error;
use std::fmt::Debug;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct WsConnectionEstablished;

impl InternalEvent for WsConnectionEstablished {
    fn emit_logs(&self) {
        debug!(message = "Connected.");
    }

    fn emit_metrics(&self) {
        counter!("connection_established_total", 1);
    }
}

#[derive(Debug)]
pub struct WsConnectionFailed<E> {
    pub error: E,
}

impl<E> InternalEvent for WsConnectionFailed<E>
where
    E: Error,
{
    fn emit_logs(&self) {
        error!(message = "Unable to connect.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct WsEventSent {
    pub byte_size: usize,
}

impl InternalEvent for WsEventSent {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct WsConnectionShutdown;

impl InternalEvent for WsConnectionShutdown {
    fn emit_logs(&self) {
        warn!(message = "Closed by the server.");
    }

    fn emit_metrics(&self) {
        counter!("connection_shutdown_total", 1);
    }
}

#[derive(Debug)]
pub struct WsConnectionError<E> {
    pub error: E,
}

impl<E> InternalEvent for WsConnectionError<E>
where
    E: Error,
{
    fn emit_logs(&self) {
        error!(message = "WebSocket connection error.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1);
    }
}
