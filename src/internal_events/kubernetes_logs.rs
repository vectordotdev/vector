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
            message = "Received one event.",
            file = %self.file
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

#[derive(Debug)]
pub struct KubernetesLogsEventAnnotationError<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventAnnotationError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to annotate event with pod metadata.",
            error_type = "event_annotation",
            event = ?self.event,
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Failed to annotate event with pod metadata.",
            "error_type" => "event_annotation",
            "stage" => "processing",
        );
        counter!("k8s_event_annotation_failures_total", 1);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsEventNamespaceAnnotationError<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventNamespaceAnnotationError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to annotate event with namespace metadata.",
            error_type = "event_annotation",
            event = ?self.event,
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Failed to annotate event with namespace metadata.",
            "error_type" => "event_annotation",
            "stage" => "processing",
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
            error_type = "parser",
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parser",
            "stage" => "processing",
        );
        counter!("k8s_docker_format_parse_failures_total", 1);
    }
}

#[derive(Debug)]
pub struct KubernetesLifecycleError<E> {
    pub message: &'static str,
    pub error: E,
}

impl<E: std::fmt::Debug + std::string::ToString + std::fmt::Display> InternalEvent
    for KubernetesLifecycleError<E>
{
    fn emit_logs(&self) {
        error!(
            message = self.message,
            error = %self.error,
            error_type = "kubernetes_lifecycle",
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "kubernetes_lifecycle",
            "stage" => "processing",
        );
    }
}
