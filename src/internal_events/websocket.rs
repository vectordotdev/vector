use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result};
use std::num::NonZeroU64;

use metrics::{counter, histogram};
use tokio_tungstenite::tungstenite::error::Error as WsError;
use vector_lib::internal_event::InternalEvent;

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

impl Display for WsKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{self:?}")
    }
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
            protocol = %self.protocol,
            kind = %self.kind
        );
        let counter = counter!(
            "component_received_bytes_total",
            "url" => self.url.to_string(),
            "protocol" => self.protocol,
            "kind" => self.kind.to_string()
        );
        counter.increment(self.byte_size as u64);
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
            kind = %self.kind
        );

        let histogram = histogram!("component_received_events_count");
        histogram.record(self.count as f64);
        let counter = counter!(
            "component_received_events_total",
            "uri" => self.url.to_string(),
            "protocol" => PROTOCOL,
            "kind" => self.kind.to_string()
        );
        counter.increment(self.count as u64);
        let counter = counter!(
            "component_received_event_bytes_total",
            "url" => self.url.to_string(),
            "protocol" => PROTOCOL,
            "kind" => self.kind.to_string()
        );
        counter.increment(self.byte_size.get() as u64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsMessageReceived")
    }
}

#[derive(Debug)]
pub struct WsReceiveError {
    pub error: WsError,
}

impl InternalEvent for WsReceiveError {
    fn emit(self) {
        error!(
            message = "Error receiving message from websocket.",
            error = %self.error,
            error_code = "ws_receive_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "ws_receive_error",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        ).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsReceiveError")
    }
}

#[derive(Debug)]
pub struct WsSendError {
    pub error: WsError,
}

impl InternalEvent for WsSendError {
    fn emit(self) {
        error!(
            message = "Error sending message to websocket.",
            error = %self.error,
            error_code = "ws_send_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "ws_send_error",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        ).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsSendError")
    }
}

#[derive(Debug)]
pub struct WsBinaryDecodeError {
    pub error: vector_lib::codecs::decoding::Error,
}

impl InternalEvent for WsBinaryDecodeError {
    fn emit(self) {
        error!(
            message = "Failed to decode binary message from websocket.",
            error = %self.error,
            error_code = "ws_binary_decode_error",
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
        );
        counter!(
            "component_errors_total",
            "error_code" => "ws_binary_decode_error",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        ).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("WsBinaryDecodeError")
    }
}

#[derive(Debug)]
pub struct PongTimeoutError {
    pub timeout_secs: NonZeroU64,
}

impl InternalEvent for PongTimeoutError {
    fn emit(self) {
        error!(
            message = "Pong not received in time.",
            timeout_secs = %self.timeout_secs,
            error_code = "pong_timeout_error",
            stage = error_stage::PROCESSING,
            error_type = error_type::CONNECTION_FAILED,
        );
        counter!(
            "component_errors_total",
            "error_code" => "pong_timeout_error",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::CONNECTION_FAILED,
        ).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("PongTimeoutError")
    }
}
