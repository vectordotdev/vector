use super::InternalEvent;
use bollard::errors::Error;
use chrono::ParseError;
use metrics::counter;

#[derive(Debug)]
pub struct DockerEventReceived<'a> {
    pub byte_size: usize,
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerEventReceived<'a> {
    fn emit_logs(&self) {
        trace!(
            message = "Received one event.",
            byte_size = %self.byte_size,
            container_id = %self.container_id
        );
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
        counter!("bytes_processed", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct DockerContainerEventReceived<'a> {
    pub container_id: &'a str,
    pub action: &'a str,
}

impl<'a> InternalEvent for DockerContainerEventReceived<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Received one container event.",
            container_id = %self.container_id,
            action = %self.action
        );
    }

    fn emit_metrics(&self) {
        counter!("container_events_processed", 1);
    }
}

#[derive(Debug)]
pub struct DockerContainerWatch<'a> {
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerContainerWatch<'a> {
    fn emit_logs(&self) {
        info!(
            message = "Started watching for logs of container.",
            container_id = %self.container_id,
        );
    }

    fn emit_metrics(&self) {
        counter!("containers_watched", 1);
    }
}

#[derive(Debug)]
pub struct DockerContainerUnwatch<'a> {
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerContainerUnwatch<'a> {
    fn emit_logs(&self) {
        info!(
            message = "Stopped watching for logs of container.",
            container_id = %self.container_id,
        );
    }

    fn emit_metrics(&self) {
        counter!("containers_unwatched", 1);
    }
}

#[derive(Debug)]
pub struct DockerCommunicationError<'a> {
    pub error: Error,
    pub container_id: Option<&'a str>,
}

impl<'a> InternalEvent for DockerCommunicationError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Error in communication with docker daemon.",
            error = %self.error,
            container_id = ?self.container_id,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("communication_errors", 1);
    }
}

#[derive(Debug)]
pub struct DockerContainerMetadataFetchFailed<'a> {
    pub error: Error,
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerContainerMetadataFetchFailed<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to fetch container metadata.",
            error = %self.error,
            container_id = ?self.container_id,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("container_metadata_fetch_errors", 1);
    }
}

#[derive(Debug)]
pub struct DockerTimestampParseFailed<'a> {
    pub error: ParseError,
    pub container_id: &'a str,
}

impl<'a> InternalEvent for DockerTimestampParseFailed<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to parse timestamp as rfc3339 timestamp.",
            error = %self.error,
            container_id = ?self.container_id,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("timestamp_parse_errors", 1);
    }
}

#[derive(Debug)]
pub struct DockerLoggingDriverUnsupported<'a> {
    pub container_id: &'a str,
    pub error: Error,
}

impl<'a> InternalEvent for DockerLoggingDriverUnsupported<'a> {
    fn emit_logs(&self) {
        error!(
            message = r#"Docker engine is not using either `jsonfile` or `journald`
                logging driver. Please enable one of these logging drivers
                to get logs from the docker daemon."#,
            error = %self.error,
            container_id = ?self.container_id,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("logging_driver_errors", 1);
    }
}
