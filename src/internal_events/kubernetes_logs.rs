use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL},
    json_size::JsonSize,
};

use crate::event::Event;

#[derive(Debug)]
pub struct KubernetesLogsEventsReceived<'a> {
    pub file: &'a str,
    pub byte_size: JsonSize,
    pub pod_info: Option<KubernetesLogsPodInfo>,
}

#[derive(Debug)]
pub struct KubernetesLogsPodInfo {
    pub name: String,
    pub namespace: String,
}

impl InternalEvent for KubernetesLogsEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
            file = %self.file,
        );
        match self.pod_info {
            Some(pod_info) => {
                let pod_name = pod_info.name;
                let pod_namespace = pod_info.namespace;

                counter!(
                    "component_received_events_total",
                    "pod_name" => pod_name.clone(),
                    "pod_namespace" => pod_namespace.clone(),
                )
                .increment(1);
                counter!(
                    "component_received_event_bytes_total",
                    "pod_name" => pod_name,
                    "pod_namespace" => pod_namespace,
                )
                .increment(self.byte_size.get() as u64);
            }
            None => {
                counter!("component_received_events_total").increment(1);
                counter!("component_received_event_bytes_total")
                    .increment(self.byte_size.get() as u64);
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
    fn emit(self) {
        error!(
            message = "Failed to annotate event with pod metadata.",
            event = ?self.event,
            error_code = ANNOTATION_FAILED,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub(crate) struct KubernetesLogsEventNamespaceAnnotationError<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventNamespaceAnnotationError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to annotate event with namespace metadata.",
            event = ?self.event,
            error_code = ANNOTATION_FAILED,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        counter!("k8s_event_namespace_annotation_failures_total").increment(1);
    }
}

#[derive(Debug)]
pub(crate) struct KubernetesLogsEventNodeAnnotationError<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventNodeAnnotationError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to annotate event with node metadata.",
            event = ?self.event,
            error_code = ANNOTATION_FAILED,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        counter!("k8s_event_node_annotation_failures_total").increment(1);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsFormatPickerEdgeCase {
    pub what: &'static str,
}

impl InternalEvent for KubernetesLogsFormatPickerEdgeCase {
    fn emit(self) {
        warn!(
            message = "Encountered format picker edge case.",
            what = %self.what,
        );
        counter!("k8s_format_picker_edge_cases_total").increment(1);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsDockerFormatParseError<'a> {
    pub error: &'a dyn std::error::Error,
}

impl InternalEvent for KubernetesLogsDockerFormatParseError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to parse log line in docker format.",
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        counter!("k8s_docker_format_parse_failures_total").increment(1);
    }
}

const KUBERNETES_LIFECYCLE: &str = "kubernetes_lifecycle";

#[derive(Debug)]
pub struct KubernetesLifecycleError<E> {
    pub message: &'static str,
    pub error: E,
    pub count: usize,
}

impl<E: std::fmt::Display> InternalEvent for KubernetesLifecycleError<E> {
    fn emit(self) {
        error!(
            message = self.message,
            error = %self.error,
            error_code = KUBERNETES_LIFECYCLE,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => KUBERNETES_LIFECYCLE,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason: self.message,
        });
    }
}
