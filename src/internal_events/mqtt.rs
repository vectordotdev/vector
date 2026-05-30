use rumqttc::ConnectionError as ConnectionErrorV3;
use rumqttc::v5::ConnectionError as ConnectionErrorV5;
use vector_lib::{
    counter,
    internal_event::{CounterName, InternalEvent, NamedInternalEvent, error_stage, error_type},
};

pub enum MqttConnectionError {
    V311 { error: ConnectionErrorV3 },
    V5 { error: ConnectionErrorV5 },
}

impl NamedInternalEvent for MqttConnectionError {
    fn name(&self) -> &'static str {
        "MqttConnectionError"
    }
}

impl InternalEvent for MqttConnectionError {
    fn emit(self) {
        match self {
            MqttConnectionError::V311 { error } => {
                error!(
                    message = "MQTT v3.1.1 connection error.",
                    error = %error,
                    error_code = "mqtt_connection_error",
                    error_type = error_type::WRITER_FAILED,
                    stage = error_stage::SENDING,
                );
            }
            MqttConnectionError::V5 { error } => {
                error!(
                    message = "MQTT v5 connection error.",
                    error = %error,
                    error_code = "mqtt_connection_error",
                    error_type = error_type::WRITER_FAILED,
                    stage = error_stage::SENDING,
                );
            }
        }

        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "mqtt_connection_error",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}
