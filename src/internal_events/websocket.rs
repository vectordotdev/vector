use std::error::Error;
use std::fmt::Debug;

use metrics::counter;
use vector_lib::internal_event::InternalEvent;

use vector_lib::internal_event::{error_stage, error_type};

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
pub struct WsConnectionFailedError {
    pub error: Box<dyn Error>,
}

impl InternalEvent for WsConnectionFailedError {
    fn emit(self) {
        error!(
            message = "WebSocket connection failed.",
            error = %self.error,
            error_code = "ws_connection_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "ws_connection_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::SENDING,
        );
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
pub struct WsConnectionError {
    pub error: tokio_tungstenite::tungstenite::Error,
}

impl InternalEvent for WsConnectionError {
    fn emit(self) {
        error!(
            message = "WebSocket connection error.",
            error = %self.error,
            error_code = "ws_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "ws_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsConnectionError")
    }
}
