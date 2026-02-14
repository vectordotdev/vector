use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::{
    internal_event::{
        ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
    },
    json_size::JsonSize,
};

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesEventsReceived {
    pub byte_size: JsonSize,
}

impl InternalEvent for KubernetesEventsReceived {
    fn emit(self) {
        trace!(
            message = "Kubernetes event received.",
            count = 1,
            byte_size = %self.byte_size,
        );

        counter!("component_received_events_total").increment(1);
        counter!("component_received_event_bytes_total").increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesEventsWatchError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for KubernetesEventsWatchError<E> {
    fn emit(self) {
        error!(
            message = "Kubernetes events watcher error.",
            error = %self.error,
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: "watcher_error"
        });
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesEventsSerializationError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for KubernetesEventsSerializationError<E> {
    fn emit(self) {
        error!(
            message = "Failed to serialize Kubernetes event.",
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: "serialization_failed"
        });
    }
}
