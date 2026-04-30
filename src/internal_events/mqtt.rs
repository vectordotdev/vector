use std::fmt::Debug;

use rumqttc::ConnectionError;
use vector_common::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
pub struct MqttConnectionError {
    pub error: ConnectionError,
}

impl InternalEvent for MqttConnectionError {
    fn emit(self) {
        error!(
            message = "MQTT connection error.",
            error = %self.error,
            error_code = "mqtt_connection_error",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_code" => "mqtt_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}
