use super::InternalEvent;
use crate::Event;
use bytes::Bytes;
use metrics::counter;

#[derive(Debug)]
pub struct KubernetesLogsEventReceived<'a> {
    pub file: &'a str,
    pub byte_size: usize,
}

impl InternalEvent for KubernetesLogsEventReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Received one event.",
            file = %self.file
        );
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
        counter!("bytes_processed", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsEventAnnotationFailed<'a> {
    pub event: &'a Event,
}

impl InternalEvent for KubernetesLogsEventAnnotationFailed<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to annotate event with pod metadata.",
            event = ?self.event
        );
    }

    fn emit_metrics(&self) {
        counter!("k8s_event_annotation_failures", 1);
    }
}

#[derive(Debug)]
pub struct KubernetesLogsDockerFormatParseFailed<'a> {
    pub message: &'a Bytes,
}

impl InternalEvent for KubernetesLogsDockerFormatParseFailed<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to parse message as JSON object.",
            value = %String::from_utf8_lossy(self.message),
        );
    }

    fn emit_metrics(&self) {
        counter!("k8s_docker_format_parse_failures", 1);
    }
}
