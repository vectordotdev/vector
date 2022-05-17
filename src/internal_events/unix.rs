use std::{io::Error, path::Path};

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct UnixSocketConnectionEstablished<'a> {
    pub path: &'a std::path::Path,
}

impl InternalEvent for UnixSocketConnectionEstablished<'_> {
    fn emit(self) {
        debug!(message = "Connected.", path = ?self.path);
        counter!("connection_established_total", 1, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketConnectionError<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E: std::error::Error> InternalEvent for UnixSocketConnectionError<'_, E> {
    fn emit(self) {
        error!(
            message = "Unable to connect.",
            error = %self.error,
            path = ?self.path,
            error_code = "connection",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "connection",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("connection_failed_total", 1, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketError<'a, E> {
    pub(crate) error: &'a E,
    pub path: &'a std::path::Path,
}

impl<E: std::fmt::Display> InternalEvent for UnixSocketError<'_, E> {
    fn emit(self) {
        error!(
            message = "Unix socket error.",
            error = %self.error,
            path = ?self.path,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("connection_errors_total", 1, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketFileDeleteError<'a> {
    pub path: &'a Path,
    pub error: Error,
}

impl<'a> InternalEvent for UnixSocketFileDeleteError<'a> {
    fn emit(self) {
        error!(
            message = "Failed in deleting unix socket file.",
            path = %self.path.display(),
            error = %self.error,
            error_code = "delete_socket_file",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "delete_socket_file",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}
