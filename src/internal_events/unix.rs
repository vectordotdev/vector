// ## skip check-events ##

use std::{io::Error, path::Path};

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct UnixSocketConnectionEstablished<'a> {
    pub path: &'a std::path::Path,
}

impl InternalEvent for UnixSocketConnectionEstablished<'_> {
    fn emit_logs(&self) {
        debug!(message = "Connected.", path = ?self.path);
    }

    fn emit_metrics(&self) {
        counter!("connection_established_total", 1, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketConnectionFailed<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E> InternalEvent for UnixSocketConnectionFailed<'_, E>
where
    E: std::error::Error,
{
    fn emit_logs(&self) {
        error!(
            message = "Unable to connect.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("connection_failed_total", 1, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketError<'a, E> {
    pub error: &'a E,
    pub path: &'a std::path::Path,
}

impl<E> InternalEvent for UnixSocketError<'_, E>
where
    E: From<std::io::Error> + std::fmt::Debug + std::fmt::Display,
{
    fn emit_logs(&self) {
        debug!(
            message = "Unix socket error.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketFileDeleteError<'a> {
    pub path: &'a Path,
    pub error: Error,
}

impl<'a> InternalEvent for UnixSocketFileDeleteError<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Failed in deleting unix socket file.",
            path = %self.path.display(),
            error = %self.error,
        );
    }
}
