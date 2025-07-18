use std::error::Error;
use std::fmt::Debug;

use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

use vector_common::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

pub const PROTOCOL: &str = "websocket";

#[derive(Debug)]
pub struct WsConnectionEstablished;

impl InternalEvent for WsConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total").increment(1);
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
        Some("WsConnectionFailed")
    }
}

#[derive(Debug)]
pub struct WsConnectionShutdown;

impl InternalEvent for WsConnectionShutdown {
    fn emit(self) {
        warn!(message = "Closed by the server.");
        counter!("connection_shutdown_total").increment(1);
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
        Some("WsConnectionError")
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum WsKind {
    Ping,
    Pong,
    Text,
    Binary,
    Close,
    Frame,
}

#[derive(Debug)]
pub struct WsBytesReceived<'a> {
    pub byte_size: usize,
    pub url: &'a str,
    pub protocol: &'static str,
    pub kind: WsKind,
}

impl InternalEvent for WsBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            url = %self.url,
            protocol = %self.protocol
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "url" => self.url.to_string(),
            "protocol" => self.protocol,
        );
    }
}

#[derive(Debug)]
pub struct WsMessageReceived<'a> {
    pub count: usize,
    pub byte_size: JsonSize,
    pub url: &'a str,
    pub protocol: &'static str,
    pub kind: WsKind,
}

impl InternalEvent for WsMessageReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            url =  %self.url,
            protcol = %self.protocol,
        );

        histogram!("component_received_events_count", self.count as f64);
        counter!(
            "component_received_events_total", self.count as u64,
            "uri" => self.url.to_string(),
            "protocol" => PROTOCOL,
        );
        counter!(
            "component_received_event_bytes_total",
            self.byte_size.get() as u64,
            "url" => self.url.to_string(),
            "protocol" => PROTOCOL,
        );
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsMessageReceived")
    }
}
