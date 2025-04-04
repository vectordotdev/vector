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
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WsListenerConnectionEstablished {
    fn emit(self) {
        debug!(message = "Websocket client connected. Client count: {self.client_count}");
        counter!("connection_established_total", &self.extra_tags).increment(1);
        gauge!("active_clients", &self.extra_tags).set(self.client_count as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionEstablished")
    }
}

#[derive(Debug)]
pub struct WsListenerConnectionFailedError {
    pub error: Box<dyn Error>,
    pub extra_tags: Vec<(String, String)>,
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
        let mut all_tags = self.extra_tags.clone();
        all_tags.extend([
            ("error_code".to_string(), "ws_connection_failed".to_string()),
            (
                "error_type".to_string(),
                error_type::CONNECTION_FAILED.to_string(),
            ),
            ("stage".to_string(), error_stage::SENDING.to_string()),
        ]);
        // Tags required by `component_errors_total` are dynamically added above.
        // ## skip check-validity-events ##
        counter!("component_errors_total", &all_tags).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionFailed")
    }
}

#[derive(Debug)]
pub struct WsListenerConnectionShutdown {
    pub client_count: usize,
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WsListenerConnectionShutdown {
    fn emit(self) {
        info!(message = "Client connection closed. Client count: {self.client_count}.");
        counter!("connection_shutdown_total", &self.extra_tags).increment(1);
        gauge!("active_clients", &self.extra_tags).set(self.client_count as f64);
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
            error_code = "ws_server_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "ws_server_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerConnectionError")
    }
}

#[derive(Debug)]
pub struct WsListenerMessageSent {
    pub message_size: usize,
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WsListenerMessageSent {
    fn emit(self) {
        counter!("websocket_messages_sent_total", &self.extra_tags).increment(1);
        counter!("websocket_bytes_sent_total", &self.extra_tags)
            .increment(self.message_size as u64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsListenerMessageSent")
    }
}
