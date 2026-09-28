use std::{collections::HashMap, sync::Arc};

use metrics::Counter;
use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, INTENTIONAL, InternalEvent, UNINTENTIONAL,
        error_stage, error_type,
    },
    json_size::JsonSize,
};
use vrl::core::Value;

use crate::event::Event;

/// Upper bound on the number of pods whose counters are cached. Nodes rarely host more
/// pods than this, and the cache is cheap to rebuild.
const MAX_CACHED_PODS: usize = 10_000;

/// Handles for the `component_received_events_total` and
/// `component_received_event_bytes_total` counters of one pod.
#[derive(Debug)]
struct PodCounters {
    events: Counter,
    bytes: Counter,
}

impl PodCounters {
    fn increment(&self, byte_size: u64) {
        self.events.increment(1);
        self.bytes.increment(byte_size);
    }
}

/// Handles for the counters of every pod seen so far, keyed by namespace and then by pod
/// name.
///
/// Both counters are labeled with the name and namespace of the pod a line came from.
/// Building them for every log line rebuilds the metric key and its labels, which is a
/// large amount of short-lived allocation on a busy node, so one handle per pod is cached
/// instead.
///
/// The emitting source owns the cache and passes it to each
/// [`KubernetesLogsEventsReceived`]. It cannot be shared between sources because a handle
/// resolves the tags of the component it was created in.
#[derive(Debug, Default)]
pub struct PodCountersCache {
    unlabeled: Option<PodCounters>,
    by_namespace: HashMap<Arc<str>, HashMap<Arc<str>, PodCounters>>,
    pods: usize,
}

impl PodCountersCache {
    /// Increments the counters of `pod`, or the component-wide ones when the pod metadata
    /// is not available. `create` builds the handles when a pod is seen for the first
    /// time.
    fn increment(
        &mut self,
        pod: Option<(&str, &str)>,
        byte_size: u64,
        create: impl FnOnce(Option<(&str, &str)>) -> PodCounters,
    ) {
        let Some((pod_name, pod_namespace)) = pod else {
            self.unlabeled
                .get_or_insert_with(|| create(None))
                .increment(byte_size);
            return;
        };

        if let Some(counters) = self
            .by_namespace
            .get(pod_namespace)
            .and_then(|by_name| by_name.get(pod_name))
        {
            counters.increment(byte_size);
            return;
        }

        if self.pods >= MAX_CACHED_PODS {
            self.by_namespace.clear();
            self.pods = 0;
        }

        let counters = create(Some((pod_name, pod_namespace)));
        counters.increment(byte_size);
        self.by_namespace
            .entry(Arc::from(pod_namespace))
            .or_default()
            .insert(Arc::from(pod_name), counters);
        self.pods += 1;
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesLogsEventsReceived<'a> {
    pub file: &'a str,
    pub byte_size: JsonSize,
    /// The pod the line came from, when its file path encodes one.
    pub pod: Option<(&'a str, &'a str)>,
    /// The counters of the pods seen so far, owned by the emitting source.
    pub counters: &'a mut PodCountersCache,
}

impl InternalEvent for KubernetesLogsEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
            file = %self.file,
        );

        self.counters
            .increment(self.pod, self.byte_size.get() as u64, |pod| match pod {
                Some((pod_name, pod_namespace)) => PodCounters {
                    events: counter!(
                        CounterName::ComponentReceivedEventsTotal,
                        "pod_name" => pod_name.to_owned(),
                        "pod_namespace" => pod_namespace.to_owned(),
                    ),
                    bytes: counter!(
                        CounterName::ComponentReceivedEventBytesTotal,
                        "pod_name" => pod_name.to_owned(),
                        "pod_namespace" => pod_namespace.to_owned(),
                    ),
                },
                None => PodCounters {
                    events: counter!(CounterName::ComponentReceivedEventsTotal),
                    bytes: counter!(CounterName::ComponentReceivedEventBytesTotal),
                },
            });
    }
}

const ANNOTATION_FAILED: &str = "annotation_failed";

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        counter!(CounterName::K8sEventNamespaceAnnotationFailuresTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => ANNOTATION_FAILED,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        counter!(CounterName::K8sEventNodeAnnotationFailuresTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesLogsFormatPickerEdgeCase {
    pub what: &'static str,
}

impl InternalEvent for KubernetesLogsFormatPickerEdgeCase {
    fn emit(self) {
        warn!(
            message = "Encountered format picker edge case.",
            what = %self.what,
        );
        counter!(CounterName::K8sFormatPickerEdgeCasesTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        counter!(CounterName::K8sDockerFormatParseFailuresTotal).increment(1);
    }
}

const KUBERNETES_LIFECYCLE: &str = "kubernetes_lifecycle";

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
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

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesMergedLineTooBigError<'a> {
    pub event: &'a Value,
    pub configured_limit: usize,
    pub encountered_size_so_far: usize,
}

impl InternalEvent for KubernetesMergedLineTooBigError<'_> {
    fn emit(self) {
        error!(
            message = "Found line that exceeds max_merged_line_bytes; discarding.",
            event = ?self.event,
            configured_limit = self.configured_limit,
            encountered_size_so_far = self.encountered_size_so_far,
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "reading_line_from_kubernetes_log",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Found line that exceeds max_merged_line_bytes; discarding.",
        });
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KubernetesMergedLineTruncated {
    pub configured_limit: usize,
    pub original_size: usize,
}

impl InternalEvent for KubernetesMergedLineTruncated {
    fn emit(self) {
        warn!(
            message = "Truncated line that exceeds max_merged_line_bytes.",
            configured_limit = self.configured_limit,
            original_size = self.original_size,
            stage = error_stage::RECEIVING,
        );
        counter!(CounterName::K8sMergedLineTruncatedTotal).increment(1);
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{event::MetricValue, metrics::Controller};

    use super::*;

    #[test]
    fn pod_counters_are_labeled_and_cached() {
        vector_lib::metrics::init_test();
        let controller = Controller::get().unwrap();
        controller.reset();

        let mut counters = PodCountersCache::default();
        for _ in 0..2 {
            emit!(KubernetesLogsEventsReceived {
                file: "k8s.log",
                byte_size: JsonSize::new(100),
                pod: Some(("k8s-counters-test-a", "k8s-counters-test-ns")),
                counters: &mut counters,
            });
        }
        emit!(KubernetesLogsEventsReceived {
            file: "k8s.log",
            byte_size: JsonSize::new(50),
            pod: Some(("k8s-counters-test-b", "k8s-counters-test-ns")),
            counters: &mut counters,
        });
        emit!(KubernetesLogsEventsReceived {
            file: "k8s.log",
            byte_size: JsonSize::new(7),
            pod: None,
            counters: &mut counters,
        });

        let metrics = controller.capture_metrics();
        let counters: Vec<_> = metrics
            .iter()
            .filter(|metric| metric.name().starts_with("component_received_event"))
            .collect();

        for (pod_name, pod_namespace, events, bytes) in [
            (
                Some("k8s-counters-test-a"),
                Some("k8s-counters-test-ns"),
                2.0,
                200.0,
            ),
            (
                Some("k8s-counters-test-b"),
                Some("k8s-counters-test-ns"),
                1.0,
                50.0,
            ),
            (None, None, 1.0, 7.0),
        ] {
            for (name, value) in [
                ("component_received_events_total", events),
                ("component_received_event_bytes_total", bytes),
            ] {
                let metric = counters
                    .iter()
                    .find(|metric| {
                        metric.name() == name
                            && metric.tags().and_then(|tags| tags.get("pod_name")) == pod_name
                            && metric.tags().and_then(|tags| tags.get("pod_namespace"))
                                == pod_namespace
                    })
                    .unwrap_or_else(|| panic!("missing {name} for {pod_name:?}"));
                assert_eq!(metric.value(), &MetricValue::Counter { value });
            }
        }
    }
}
