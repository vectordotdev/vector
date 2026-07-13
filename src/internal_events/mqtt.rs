use rumqttc::ConnectionError as ConnectionErrorV3;
use rumqttc::v5::ConnectionError as ConnectionErrorV5;
use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        CounterName, InternalEvent, NamedInternalEvent as NamedInternalEventTrait, error_stage,
        error_type,
    },
};

/// Direction of an MQTT operation, used to derive the right `error_type` and
/// `stage` tags on emitted metrics.
#[derive(Clone, Copy)]
#[allow(
    dead_code,
    reason = "Source and Sink variants are gated by feature flags."
)]
pub enum MqttDirection {
    Source,
    Sink,
}

impl MqttDirection {
    const fn error_type(self) -> &'static str {
        match self {
            MqttDirection::Source => error_type::READER_FAILED,
            MqttDirection::Sink => error_type::WRITER_FAILED,
        }
    }

    const fn stage(self) -> &'static str {
        match self {
            MqttDirection::Source => error_stage::RECEIVING,
            MqttDirection::Sink => error_stage::SENDING,
        }
    }
}

pub enum MqttConnectionError {
    V311 {
        direction: MqttDirection,
        error: ConnectionErrorV3,
    },
    V5 {
        direction: MqttDirection,
        error: ConnectionErrorV5,
    },
}

impl NamedInternalEventTrait for MqttConnectionError {
    fn name(&self) -> &'static str {
        "MqttConnectionError"
    }
}

impl InternalEvent for MqttConnectionError {
    fn emit(self) {
        let direction = match &self {
            MqttConnectionError::V311 { direction, error } => {
                error!(
                    message = "MQTT v3.1.1 connection error.",
                    error = %error,
                    error_code = "mqtt_connection_error",
                    error_type = direction.error_type(),
                    stage = direction.stage(),
                );
                *direction
            }
            MqttConnectionError::V5 { direction, error } => {
                error!(
                    message = "MQTT v5 connection error.",
                    error = %error,
                    error_code = "mqtt_connection_error",
                    error_type = direction.error_type(),
                    stage = direction.stage(),
                );
                *direction
            }
        };

        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "mqtt_connection_error",
            "error_type" => direction.error_type(),
            "stage" => direction.stage(),
        )
        .increment(1);
    }
}

#[allow(dead_code, reason = "Only used by the mqtt source.")]
pub struct MqttSubscribeError {
    pub topic: String,
    pub error: String,
}

impl NamedInternalEventTrait for MqttSubscribeError {
    fn name(&self) -> &'static str {
        "MqttSubscribeError"
    }
}

impl InternalEvent for MqttSubscribeError {
    fn emit(self) {
        error!(
            message = "Failed to subscribe to MQTT topic.",
            topic = %self.topic,
            error = %self.error,
            error_code = "mqtt_subscribe_error",
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "mqtt_subscribe_error",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[allow(dead_code, reason = "Only used by the mqtt source.")]
pub struct MqttAckError {
    pub error: String,
}

impl NamedInternalEventTrait for MqttAckError {
    fn name(&self) -> &'static str {
        "MqttAckError"
    }
}

impl InternalEvent for MqttAckError {
    fn emit(self) {
        error!(
            message = "Failed to send MQTT acknowledgement.",
            error = %self.error,
            error_code = "mqtt_ack_error",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "mqtt_ack_error",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct MqttConnectionShutdown;

impl InternalEvent for MqttConnectionShutdown {
    fn emit(self) {
        debug!(message = "MQTT connection closed.");
        counter!(CounterName::ConnectionShutdownTotal).increment(1);
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::internal_event::{error_stage, error_type};

    use super::MqttDirection;

    #[test]
    fn direction_source_uses_reader_failed_and_receiving() {
        assert_eq!(
            MqttDirection::Source.error_type(),
            error_type::READER_FAILED
        );
        assert_eq!(MqttDirection::Source.stage(), error_stage::RECEIVING);
    }

    #[test]
    fn direction_sink_uses_writer_failed_and_sending() {
        assert_eq!(MqttDirection::Sink.error_type(), error_type::WRITER_FAILED);
        assert_eq!(MqttDirection::Sink.stage(), error_stage::SENDING);
    }
}
