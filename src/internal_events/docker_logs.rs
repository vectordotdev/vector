use super::prelude::{error_stage, error_type};
use bollard::errors::Error;
use chrono::ParseError;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DockerLogsEventsReceived<'a> {
    pub byte_size: usize,
    pub container_id: &'a str,
    pub container_name: &'a str,
}

impl<'a> InternalEvent for DockerLogsEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
            container_id = %self.container_id
        );
        counter!(
            "component_received_events_total", 1,
            "container_name" => self.container_name.to_owned()
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "container_name" => self.container_name.to_owned()
        );
        // deprecated
        counter!(
            "events_in_total", 1,
            "container_name" => self.container_name.to_owned()
        );
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerEventReceived<'a> {
    pub container_id: &'a str,
    pub action: &'a str,
}

impl<'a> InternalEvent for DockerLogsContainerEventReceived<'a> {
    fn emit(self) {
        debug!(
            message = "Received one container event.",
            container_id = %self.container_id,
            action = %self.action,
        );
        counter!("container_processed_events_total", 1);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerWatch<'a> {
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerLogsContainerWatch<'a> {
    fn emit(self) {
        info!(
            message = "Started watching for container logs.",
            container_id = %self.container_id,
        );
        counter!("containers_watched_total", 1);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerUnwatch<'a> {
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerLogsContainerUnwatch<'a> {
    fn emit(self) {
        info!(
            message = "Stopped watching for container logs.",
            container_id = %self.container_id,
        );
        counter!("containers_unwatched_total", 1);
    }
}

#[derive(Debug)]
pub struct DockerLogsCommunicationError<'a> {
    pub error: Error,
    pub container_id: Option<&'a str>,
}

impl<'a> InternalEvent for DockerLogsCommunicationError<'a> {
    fn emit(self) {
        error!(
            message = "Error in communication with Docker daemon.",
            error = ?self.error,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            container_id = ?self.container_id,
            internal_log_rate_secs = 10
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("communication_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct DockerLogsContainerMetadataFetchError<'a> {
    pub error: Error,
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerLogsContainerMetadataFetchError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to fetch container metadata.",
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            container_id = ?self.container_id,
            internal_log_rate_secs = 10
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
            "container_id" => self.container_id.to_owned(),
        );
        // deprecated
        counter!("container_metadata_fetch_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct DockerLogsTimestampParseError<'a> {
    pub error: ParseError,
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerLogsTimestampParseError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to parse timestamp as RFC3339 timestamp.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            container_id = ?self.container_id,
            internal_log_rate_secs = 10
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "container_id" => self.container_id.to_owned(),
        );
        // deprecated
        counter!("timestamp_parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct DockerLogsLoggingDriverUnsupportedError<'a> {
    pub container_id: &'a str,
    pub error: Error,
}

impl<'a> InternalEvent for DockerLogsLoggingDriverUnsupportedError<'a> {
    fn emit(self) {
        error!(
            message = "Docker engine is not using either the `jsonfile` or `journald` logging driver. Please enable one of these logging drivers to get logs from the Docker daemon.",
            error = ?self.error,
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::RECEIVING,
            container_id = ?self.container_id,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::RECEIVING,
            "container_id" => self.container_id.to_owned(),
        );
        // deprecated
        counter!("logging_driver_errors_total", 1);
    }
}
