use std::time::Duration;

use metrics::{counter, histogram};
use tokio::time::error::Elapsed;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ExecEventsReceived<'a> {
    pub count: usize,
    pub command: &'a str,
    pub byte_size: usize,
}

impl InternalEvent for ExecEventsReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Received events.",
            count = self.count,
            command = %self.command,
            byte_size = self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_events_total", self.count as u64,
            "command" => self.command.to_owned(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "command" => self.command.to_owned(),
        );
        // deprecated
        counter!(
            "events_in_total", self.count as u64,
            "command" => self.command.to_owned(),
        );
        counter!(
            "processed_bytes_total", self.byte_size as u64,
            "command" => self.command.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct ExecFailedError<'a> {
    pub command: &'a str,
    pub error: std::io::Error,
}

impl InternalEvent for ExecFailedError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Unable to exec.",
            command = %self.command,
            error = ?self.error,
            error_type = "command_failed",
            stage = "receiving",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "command" => self.command.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => "command_failed",
            "stage" => "receiving",
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "command" => self.command.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => "command_failed",
            "stage" => "receiving",
        );
    }
}

#[derive(Debug)]
pub struct ExecTimeoutError<'a> {
    pub command: &'a str,
    pub elapsed_seconds: u64,
    pub error: Elapsed,
}

impl InternalEvent for ExecTimeoutError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Timeout during exec.",
            command = %self.command,
            elapsed_seconds = %self.elapsed_seconds,
            error = %self.error,
            error_type = "timed_out",
            stage = "receiving",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "command" => self.command.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => "timed_out",
            "stage" => "receiving",
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "command" => self.command.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => "timed_out",
            "stage" => "receiving",
        );
    }
}

#[derive(Debug)]
pub struct ExecCommandExecuted<'a> {
    pub command: &'a str,
    pub exit_status: Option<i32>,
    pub exec_duration: Duration,
}

impl ExecCommandExecuted<'_> {
    fn exit_status_string(&self) -> String {
        match self.exit_status {
            Some(exit_status) => exit_status.to_string(),
            None => "unknown".to_string(),
        }
    }
}

impl InternalEvent for ExecCommandExecuted<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Executed command.",
            command = %self.command,
            exit_status = %self.exit_status_string(),
            elapsed_millis = %self.exec_duration.as_millis(),
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "command_executed_total", 1,
            "command" => self.command.to_owned(),
            "exit_status" => self.exit_status_string(),
        );

        histogram!(
            "command_execution_duration_seconds", self.exec_duration,
            "command" => self.command.to_owned(),
            "exit_status" => self.exit_status_string(),
        );
    }
}
