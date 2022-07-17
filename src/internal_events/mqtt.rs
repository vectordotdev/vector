use std::fmt::Debug;

use metrics::counter;
use rumqttc::{ClientError, ConnectionError};
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct MqttConnectionShutdown;

impl InternalEvent for MqttConnectionShutdown {
    fn emit(self) {
        warn!(message = "Closed by the server.");
        counter!("connection_shutdown_total", 1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("MqttConnectionShutdown")
    }
}

#[derive(Debug)]
pub struct MqttConnectionError {
    pub error: ConnectionError,
}

impl InternalEvent for MqttConnectionError {
    fn emit(self) {
        error!(
            message = "MQTT connection error.",
            error = %self.error,
            // error_code = "ws_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total", 1,
            // "error_code" => "ws_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
    }

    fn name(&self) -> Option<&'static str> {
        Some("MqttConnectionError")
    }
}

#[derive(Debug)]
pub struct MqttClientError {
    pub error: ClientError,
}

impl InternalEvent for MqttClientError {
    fn emit(self) {
        error!(
            message = "MQTT client error.",
            error = %self.error,
            // error_code = "ws_client_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total", 1,
            // "error_code" => "ws_client_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
    }

    fn name(&self) -> Option<&'static str> {
        Some("MqttClientError")
    }
}
