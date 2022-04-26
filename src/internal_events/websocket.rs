use metrics::counter;
use std::error::Error;
use std::fmt::Debug;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct WsConnectionEstablished;

impl InternalEvent for WsConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total", 1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsConnectionEstablished")
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
    fn emit(self) {
        error!(message = "Unable to connect.", error = %self.error);
        counter!("connection_failed_total", 1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsConnectionFailed")
    }
}

#[derive(Debug)]
pub struct WsConnectionShutdown;

impl InternalEvent for WsConnectionShutdown {
    fn emit(self) {
        warn!(message = "Closed by the server.");
        counter!("connection_shutdown_total", 1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsConnectionShutdown")
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
    fn emit(self) {
        error!(message = "WebSocket connection error.", error = %self.error);
        counter!("connection_errors_total", 1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsConnectionError")
    }
}
