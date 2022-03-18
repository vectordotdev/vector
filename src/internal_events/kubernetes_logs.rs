use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::event::Event;

#[derive(Debug)]
pub struct KubernetesLogsEventsReceived<'a> {
    pub file: &'a str,
    pub byte_size: usize,
    pub pod_name: Option<&'a str>,
}

impl InternalEvent for KubernetesLogsEventsReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
            file = %self.file,
        );
    }

    fn emit_metrics(&self) {
        match self.pod_name {
            Some(name) => {
                counter!("component_received_events_total", 1, "pod_name" => name.to_owned());
                counter!("component_received_event_bytes_total", self.byte_size as u64, "pod_name" => name.to_owned());
                counter!("events_in_total", 1, "pod_name" => name.to_owned());
                counter!(
                    "processed_bytes_total", self.byte_size as u64,
                    "pod_name" => name.to_owned()
                );
            }
            None => {
                counter!("component_received_events_total", 1);
                counter!(
                    "component_received_event_bytes_total",
                    self.byte_size as u64
                );
                counter!("events_in_total", 1);
                counter!("processed_bytes_total", self.byte_size as u64);
            }
        }
    }
}

const ANNOTATION_FAILED: &str = "annotation_failed";

#[derive(Debug)]
pub struct KubernetesLogsEventAnnotationError<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventAnnotationError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to annotate event with pod metadata.",
            event = ?self.event,
            error_code = ANNOTATION_FAILED,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        counter!("k8s_event_annotation_failures_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct KubernetesLogsEventNamespaceAnnotationError<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventNamespaceAnnotationError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to annotate event with namespace metadata.",
            event = ?self.event,
            error_code = ANNOTATION_FAILED,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        counter!("k8s_event_namespace_annotation_failures_total", 1);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsFormatPickerEdgeCase {
    pub what: &'static str,
}

impl InternalEvent for KubernetesLogsFormatPickerEdgeCase {
    fn emit_logs(&self) {
        warn!(
            message = "Encountered format picker edge case.",
            what = %self.what,
        );
    }

    fn emit_metrics(&self) {
        counter!("k8s_format_picker_edge_cases_total", 1);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsDockerFormatParseError<'a> {
    pub error: &'a dyn std::error::Error,
}

impl InternalEvent for KubernetesLogsDockerFormatParseError<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to parse log line in docker format.",
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        counter!("k8s_docker_format_parse_failures_total", 1);
    }
}

const KUBERNETES_LIFECYCLE: &str = "kubernetes_lifecycle";

#[derive(Debug)]
pub struct KubernetesLifecycleError<E> {
    pub message: &'static str,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for KubernetesLifecycleError<E> {
    fn emit_logs(&self) {
        error!(
            message = self.message,
            error = %self.error,
            error_code = KUBERNETES_LIFECYCLE,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => KUBERNETES_LIFECYCLE,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}
