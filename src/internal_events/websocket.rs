#![allow(dead_code)] // TODO requires optional feature compilation

use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result},
};

use metrics::{counter, histogram};
use tokio_tungstenite::tungstenite::error::Error as TungsteniteError;
use vector_common::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::InternalEvent;

pub const PROTOCOL: &str = "websocket";

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketConnectionEstablished;

impl InternalEvent for WebSocketConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketConnectionFailedError {
    pub error: Box<dyn Error>,
}

impl InternalEvent for WebSocketConnectionFailedError {
    fn emit(self) {
        error!(
            message = "WebSocket connection failed.",
            error = %self.error,
            error_code = "websocket_connection_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "protocol" => PROTOCOL,
            "error_code" => "websocket_connection_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketConnectionShutdown;

impl InternalEvent for WebSocketConnectionShutdown {
    fn emit(self) {
        warn!(message = "Closed by the server.");
        counter!("connection_shutdown_total").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketConnectionError {
    pub error: tokio_tungstenite::tungstenite::Error,
}

impl InternalEvent for WebSocketConnectionError {
    fn emit(self) {
        error!(
            message = "WebSocket connection error.",
            error = %self.error,
            error_code = "websocket_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "protocol" => PROTOCOL,
            "error_code" => "websocket_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub enum WebSocketKind {
    Ping,
    Pong,
    Text,
    Binary,
    Close,
    Frame,
}

impl Display for WebSocketKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketBytesReceived<'a> {
    pub byte_size: usize,
    pub url: &'a str,
    pub protocol: &'static str,
    pub kind: WebSocketKind,
}

impl InternalEvent for WebSocketBytesReceived<'_> {
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

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketMessageReceived<'a> {
    pub count: usize,
    pub byte_size: JsonSize,
    pub url: &'a str,
    pub protocol: &'static str,
    pub kind: WebSocketKind,
}

impl InternalEvent for WebSocketMessageReceived<'_> {
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
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketReceiveError<'a> {
    pub error: &'a TungsteniteError,
}

impl InternalEvent for WebSocketReceiveError<'_> {
    fn emit(self) {
        error!(
            message = "Error receiving message from websocket.",
            error = %self.error,
            error_code = "websocket_receive_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "protocol" => PROTOCOL,
            "error_code" => "websocket_receive_error",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct WebSocketSendError<'a> {
    pub error: &'a TungsteniteError,
}

impl InternalEvent for WebSocketSendError<'_> {
    fn emit(self) {
        error!(
            message = "Error sending message to websocket.",
            error = %self.error,
            error_code = "websocket_send_error",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "protocol" => PROTOCOL,
            "error_code" => "websocket_send_error",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
