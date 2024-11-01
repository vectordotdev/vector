use bollard::errors::Error;
use chrono::ParseError;
use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

#[derive(Debug)]
pub struct DockerLogsEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub container_id: &'a str,
    pub container_name: &'a str,
}

impl InternalEvent for DockerLogsEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
            container_id = %self.container_id
        );
        counter!(
            "component_received_events_total", "container_name" => self.container_name.to_owned()
        )
        .increment(1);
        counter!(
            "component_received_event_bytes_total", "container_name" => self.container_name.to_owned()
        ).increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerEventReceived<'a> {
    pub container_id: &'a str,
    pub action: &'a str,
}

impl InternalEvent for DockerLogsContainerEventReceived<'_> {
    fn emit(self) {
        debug!(
            message = "Received one container event.",
            container_id = %self.container_id,
            action = %self.action,
        );
        counter!("container_processed_events_total").increment(1);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerWatch<'a> {
    pub container_id: &'a str,
}

impl InternalEvent for DockerLogsContainerWatch<'_> {
    fn emit(self) {
        info!(
            message = "Started watching for container logs.",
            container_id = %self.container_id,
        );
        counter!("containers_watched_total").increment(1);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerUnwatch<'a> {
    pub container_id: &'a str,
}

impl InternalEvent for DockerLogsContainerUnwatch<'_> {
    fn emit(self) {
        info!(
            message = "Stopped watching for container logs.",
            container_id = %self.container_id,
        );
        counter!("containers_unwatched_total").increment(1);
    }
}

#[derive(Debug)]
pub struct DockerLogsCommunicationError<'a> {
    pub error: Error,
    pub container_id: Option<&'a str>,
}

impl InternalEvent for DockerLogsCommunicationError<'_> {
    fn emit(self) {
        error!(
            message = "Error in communication with Docker daemon.",
            error = ?self.error,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            container_id = ?self.container_id,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerMetadataFetchError<'a> {
    pub error: Error,
    pub container_id: &'a str,
}

impl InternalEvent for DockerLogsContainerMetadataFetchError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to fetch container metadata.",
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            container_id = ?self.container_id,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
            "container_id" => self.container_id.to_owned(),
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct DockerLogsTimestampParseError<'a> {
    pub error: ParseError,
    pub container_id: &'a str,
}

impl InternalEvent for DockerLogsTimestampParseError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to parse timestamp as RFC3339 timestamp.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            container_id = ?self.container_id,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "container_id" => self.container_id.to_owned(),
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct DockerLogsLoggingDriverUnsupportedError<'a> {
    pub container_id: &'a str,
    pub error: Error,
}

impl InternalEvent for DockerLogsLoggingDriverUnsupportedError<'_> {
    fn emit(self) {
        error!(
            message = "Docker engine is not using either the `jsonfile` or `journald` logging driver. Please enable one of these logging drivers to get logs from the Docker daemon.",
            error = ?self.error,
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::RECEIVING,
            container_id = ?self.container_id,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::RECEIVING,
            "container_id" => self.container_id.to_owned(),
        )
        .increment(1);
    }
}
