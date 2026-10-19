use std::fmt::Debug;

use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

#[cfg(feature = "sinks-mqtt")]
#[derive(Debug, NamedInternalEvent)]
pub struct MqttConnectionError {
    pub error: rumqttc::ConnectionError,
}

#[cfg(feature = "sinks-mqtt")]
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

#[cfg(feature = "sources-mqtt")]
#[derive(Debug, NamedInternalEvent)]
pub struct MqttSubscribeError {
    pub error: rumqttc::ClientError,
}

#[cfg(feature = "sources-mqtt")]
impl InternalEvent for MqttSubscribeError {
    fn emit(self) {
        error!(
            message = "MQTT subscribe error.",
            error = %self.error,
            error_code = "mqtt_subscribe_error",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "mqtt_subscribe_error",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
