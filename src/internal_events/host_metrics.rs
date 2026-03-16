use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
pub struct HostMetricsScrapeError {
    pub message: &'static str,
}

impl InternalEvent for HostMetricsScrapeError {
    fn emit(self) {
        error!(
            message = self.message,
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );

        counter!(
            "component_errors_total",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct HostMetricsScrapeDetailError<E> {
    pub message: &'static str,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for HostMetricsScrapeDetailError<E> {
    fn emit(self) {
        error!(
            message = self.message,
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
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct HostMetricsScrapeFilesystemError {
    pub message: &'static str,
    pub error: heim::Error,
    pub mount_point: String,
}

impl InternalEvent for HostMetricsScrapeFilesystemError {
    fn emit(self) {
        error!(
            message = self.message,
            mount_point = self.mount_point,
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
    }
}
