//! Internal event and error types for NetFlow source.

use vector_lib::internal_event::{InternalEvent, error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};
use metrics::counter;
use std::net::SocketAddr;

#[derive(Debug)]
pub struct NetflowParseError<'a> {
    pub error: &'a str,
    pub protocol: &'a str,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for NetflowParseError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to parse NetFlow packet.",
            error = %self.error,
            protocol = %self.protocol,
            peer_addr = %self.peer_addr,
            error_code = "parse_failed",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "parse_failed",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "protocol" => self.protocol.to_string(),
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct NetflowTemplateError<'a> {
    pub error: &'a str,
    pub template_id: u16,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for NetflowTemplateError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to process NetFlow template.",
            error = %self.error,
            template_id = %self.template_id,
            peer_addr = %self.peer_addr,
            error_code = "template_error",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "template_error",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct NetflowFieldParseError<'a> {
    pub error: &'a str,
    pub field_type: u16,
    pub template_id: u16,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for NetflowFieldParseError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to parse NetFlow field.",
            error = %self.error,
            field_type = %self.field_type,
            template_id = %self.template_id,
            peer_addr = %self.peer_addr,
            error_code = "field_parse_error",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "field_parse_error",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct NetflowEventsDropped {
    pub count: usize,
    pub reason: &'static str,
}

impl InternalEvent for NetflowEventsDropped {
    fn emit(self) {
        error!(
            message = "NetFlow events dropped.",
            count = %self.count,
            reason = %self.reason,
            error_code = "events_dropped",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "events_dropped",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason: self.reason,
        });
    }
} 