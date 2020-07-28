use super::InternalEvent;
use crate::Event;
use metrics::counter;

#[derive(Debug)]
pub struct KubernetesLogsEventReceived<'a> {
    pub file: &'a str,
    pub byte_size: usize,
}

impl InternalEvent for KubernetesLogsEventReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "received one event",
            file = %self.file
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "source",
            "component_type" => "kubernetes_logs",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "kubernetes_logs",
        );
    }
}

#[derive(Debug)]
pub struct KubernetesLogsEventAnnotationFailed<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventAnnotationFailed<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "failed to annotate event with pod metadata",
            event = ?self.event
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "k8s_event_annotation_failures", 1,
            "component_kind" => "source",
            "component_type" => "kubernetes_logs",
        );
    }
}
