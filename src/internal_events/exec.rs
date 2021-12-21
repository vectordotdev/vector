// ## skip check-events ##

use std::time::Duration;

use metrics::{counter, histogram};
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
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_events_total", self.count as u64,
            "command" => self.command.to_owned(),
        );
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
pub struct ExecFailed<'a> {
    pub command: &'a str,
    pub error: std::io::Error,
}

impl InternalEvent for ExecFailed<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Unable to exec.",
            command = %self.command,
            error = ?self.error,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors_total", 1,
            "command" => self.command.to_owned(),
            "error_type" => "failed",
        );
    }
}

#[derive(Debug)]
pub struct ExecTimeout<'a> {
    pub command: &'a str,
    pub elapsed_seconds: u64,
}

impl InternalEvent for ExecTimeout<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Timeout during exec.",
            command = %self.command,
            elapsed_seconds = %self.elapsed_seconds
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors_total", 1,
            "command" => self.command.to_owned(),
            "error_type" => "timed_out",
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
