use std::error::Error;
use std::fmt::Debug;

use futures::channel::mpsc::TrySendError;
use metrics::{counter, gauge};
use tokio_tungstenite::tungstenite::Message;
use vector_lib::internal_event::InternalEvent;

use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct WsListenerConnectionEstablished {
    pub client_count: usize,
}

impl InternalEvent for WsListenerConnectionEstablished {
    fn emit(self) {
        debug!(message = "Websocket client connected.");
        counter!("connection_established_total").increment(1);
        gauge!("active_clients").set(self.client_count as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionEstablished")
    }
}

#[derive(Debug)]
pub struct WsListenerConnectionFailedError {
    pub error: Box<dyn Error>,
}

impl InternalEvent for WsListenerConnectionFailedError {
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
            "component_errors_total",
            "error_code" => "ws_connection_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionFailed")
    }
}

#[derive(Debug)]
pub struct WsListenerConnectionShutdown {
    pub client_count: usize,
}

impl InternalEvent for WsListenerConnectionShutdown {
    fn emit(self) {
        warn!(message = "Client connection closed.");
        counter!("connection_shutdown_total").increment(1);
        gauge!("active_clients").set(self.client_count as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionShutdown")
    }
}

#[derive(Debug)]
pub struct WsListenerSendError {
    pub error: TrySendError<Message>,
}

impl InternalEvent for WsListenerSendError {
    fn emit(self) {
        error!(
            message = "WebSocket message send error.",
            error = %self.error,
            error_code = "ws_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "ws_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionError")
    }
}
