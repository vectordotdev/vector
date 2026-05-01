use std::fmt::Debug;

use rumqttc::ConnectionError;
use vector_lib::internal_event::{CounterName, InternalEvent, error_stage, error_type};
use vector_lib::{NamedInternalEvent, counter};

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
            CounterName::ComponentErrorsTotal,
            "error_code" => "mqtt_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}
