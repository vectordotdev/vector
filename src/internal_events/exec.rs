use std::time::Duration;

use metrics::{counter, histogram};
use tokio::time::error::Elapsed;
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL},
    json_size::JsonSize,
};

use super::prelude::io_error_code;

#[derive(Debug)]
pub struct ExecEventsReceived<'a> {
    pub count: usize,
    pub command: &'a str,
    pub byte_size: JsonSize,
}

impl InternalEvent for ExecEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size.get(),
            command = %self.command,
        );
        counter!(
            "component_received_events_total", self.count as u64,
            "command" => self.command.to_owned(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size.get() as u64,
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
    fn emit(self) {
        error!(
            message = "Unable to exec.",
            command = %self.command,
            error = ?self.error,
            error_type = error_type::COMMAND_FAILED,
            error_code = %io_error_code(&self.error),
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "command" => self.command.to_owned(),
            "error_type" => error_type::COMMAND_FAILED,
            "error_code" => io_error_code(&self.error),
            "stage" => error_stage::RECEIVING,
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
    fn emit(self) {
        error!(
            message = "Timeout during exec.",
            command = %self.command,
            elapsed_seconds = %self.elapsed_seconds,
            error = %self.error,
            error_type = error_type::TIMED_OUT,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "command" => self.command.to_owned(),
            "error_type" => error_type::TIMED_OUT,
            "stage" => error_stage::RECEIVING,
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
    fn emit(self) {
        let exit_status = self.exit_status_string();
        trace!(
            message = "Executed command.",
            command = %self.command,
            exit_status = %exit_status,
            elapsed_millis = %self.exec_duration.as_millis(),
            internal_log_rate_limit = true,
        );
        counter!(
            "command_executed_total", 1,
            "command" => self.command.to_owned(),
            "exit_status" => exit_status.clone(),
        );

        histogram!(
            "command_execution_duration_seconds", self.exec_duration,
            "command" => self.command.to_owned(),
            "exit_status" => exit_status,
        );
    }
}

pub enum ExecFailedToSignalChild {
    #[cfg(unix)]
    SignalError(nix::errno::Errno),
    #[cfg(unix)]
    FailedToMarshalPid(std::num::TryFromIntError),
    #[cfg(unix)]
    NoPid,
    #[cfg(windows)]
    IoError(std::io::Error),
}

impl ExecFailedToSignalChild {
    fn to_error_code(&self) -> String {
        use ExecFailedToSignalChild::*;

        match self {
            #[cfg(unix)]
            SignalError(err) => format!("errno_{}", err),
            #[cfg(unix)]
            FailedToMarshalPid(_) => String::from("failed_to_marshal_pid"),
            #[cfg(unix)]
            NoPid => String::from("no_pid"),
            #[cfg(windows)]
            IoError(err) => err.to_string(),
        }
    }
}

impl std::fmt::Display for ExecFailedToSignalChild {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use ExecFailedToSignalChild::*;

        match self {
            #[cfg(unix)]
            SignalError(err) => write!(f, "errno: {}", err),
            #[cfg(unix)]
            FailedToMarshalPid(err) => write!(f, "failed to marshal pid to i32: {}", err),
            #[cfg(unix)]
            NoPid => write!(f, "child had no pid"),
            #[cfg(windows)]
            IoError(err) => write!(f, "io error: {}", err),
        }
    }
}

pub struct ExecFailedToSignalChildError<'a> {
    pub command: &'a tokio::process::Command,
    pub error: ExecFailedToSignalChild,
}

impl InternalEvent for ExecFailedToSignalChildError<'_> {
    fn emit(self) {
        error!(
            message = %format!("Failed to send SIGTERM to child, aborting early: {}", self.error),
            command = ?self.command.as_std(),
            error_code = %self.error.to_error_code(),
            error_type = error_type::COMMAND_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "command" => format!("{:?}", self.command.as_std()),
            "error_code" => self.error.to_error_code(),
            "error_type" => error_type::COMMAND_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

pub struct ExecChannelClosedError;

impl InternalEvent for ExecChannelClosedError {
    fn emit(self) {
        let exec_reason = "Receive channel closed, unable to send.";
        error!(
            message = exec_reason,
            error_type = error_type::COMMAND_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::COMMAND_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: exec_reason
        });
    }
}
