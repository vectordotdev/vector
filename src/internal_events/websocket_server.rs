use std::{error::Error, fmt::Debug};

use vector_lib::{
    NamedInternalEvent, counter, gauge,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketListenerConnectionEstablished {
    pub client_count: usize,
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WebSocketListenerConnectionEstablished {
    fn emit(self) {
        debug!(
            message = format!(
                "Websocket client connected. Client count: {}",
                self.client_count
            )
        );
        counter!(CounterName::ConnectionEstablishedTotal, &self.extra_tags).increment(1);
        gauge!(CounterName::ActiveClients, &self.extra_tags).set(self.client_count as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketListenerConnectionFailedError {
    pub error: Box<dyn Error>,
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WebSocketListenerConnectionFailedError {
    fn emit(self) {
        error!(
            message = "WebSocket connection failed.",
            error = %self.error,
            error_code = "ws_connection_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::SENDING,
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
        counter!(CounterName::ComponentErrorsTotal, &all_tags).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketListenerConnectionShutdown {
    pub client_count: usize,
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WebSocketListenerConnectionShutdown {
    fn emit(self) {
        info!(
            message = format!(
                "Client connection closed. Client count: {}.",
                self.client_count
            )
        );
        counter!(CounterName::ConnectionShutdownTotal, &self.extra_tags).increment(1);
        gauge!(CounterName::ActiveClients, &self.extra_tags).set(self.client_count as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketListenerSendError {
    pub error: Box<dyn Error>,
}

impl InternalEvent for WebSocketListenerSendError {
    fn emit(self) {
        error!(
            message = "WebSocket message send error.",
            error = %self.error,
            error_code = "ws_server_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "ws_server_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketListenerMessageSent {
    pub message_size: usize,
    pub extra_tags: Vec<(String, String)>,
}

impl InternalEvent for WebSocketListenerMessageSent {
    fn emit(self) {
        counter!(CounterName::WebsocketMessagesSentTotal, &self.extra_tags).increment(1);
        counter!(CounterName::WebsocketBytesSentTotal, &self.extra_tags)
            .increment(self.message_size as u64);
    }
}
