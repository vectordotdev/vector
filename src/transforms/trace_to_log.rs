use vector_lib::config::clone_input_definitions;
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::metadata_path;

use crate::config::OutputId;
use crate::{
    config::{DataType, GenerateConfig, Input, TransformConfig, TransformContext, TransformOutput},
    event::{Event, LogEvent, TRACE_LAYOUT_KEY},
    schema::Definition,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `trace_to_log` transform.
///
/// This is a naive implementation that simply converts a `TraceEvent` to a `LogEvent`.
/// The conversion preserves all trace attributes (span IDs, trace IDs, etc.) as log fields without modification.
/// This will need to be updated when Vector's trace data model is finalized to properly handle trace-specific semantics and field mappings.
#[configurable_component(transform("trace_to_log", "Convert trace events to log events."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct TraceToLogConfig {
    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

impl GenerateConfig for TraceToLogConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            log_namespace: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "trace_to_log")]
impl TransformConfig for TraceToLogConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(TraceToLog))
    }

    fn enable_concurrency(&self) -> bool {
        true
    }

    fn input(&self) -> Input {
        Input::trace()
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, Definition)],
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        )]
    }
}

#[derive(Clone, Debug)]
pub struct TraceToLog;

impl FunctionTransform for TraceToLog {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        if let Event::Trace(trace) = event {
            let mut log = LogEvent::from(trace);
            // The layout marker is only meaningful to components that consume
            // traces as traces. Drop it so a converted log is not classified
            // as Vector-namespaced solely because of `%vector.trace_layout`.
            log.remove_prune(metadata_path!("vector", TRACE_LAYOUT_KEY), true);
            output.push(Event::Log(log));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::components::assert_transform_compliance;
    use crate::transforms::test::create_topology;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::{
        config::LogNamespace,
        event::{TRACE_LAYOUT_DATADOG, TraceEvent},
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<TraceToLogConfig>();
    }

    async fn do_transform(trace: TraceEvent) -> Option<LogEvent> {
        assert_transform_compliance(async move {
            let config = TraceToLogConfig {
                log_namespace: Some(false),
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            tx.send(trace.into()).await.unwrap();

            let result = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);

            result
        })
        .await
        .map(|e| e.into_log())
    }

    #[tokio::test]
    async fn transform_trace() {
        use vrl::btreemap;

        let trace = TraceEvent::from(btreemap! {
            "span_id" => "abc123",
            "trace_id" => "xyz789",
            "span_name" => "test-span",
            "service" => "my-service",
        });

        let (expected_map, _) = trace.clone().into_parts();

        let log = do_transform(trace).await.unwrap();
        let (actual_value, _) = log.into_parts();
        let actual_map = actual_value
            .into_object()
            .expect("log value should be an object");

        assert_eq!(
            actual_map, expected_map,
            "Trace data fields should be preserved"
        );
    }

    #[tokio::test]
    async fn drops_trace_layout_marker() {
        use vrl::btreemap;

        let mut trace = TraceEvent::from(btreemap! {
            "host" => "a_hostname",
            "span_id" => "abc123",
        });
        trace.metadata_mut().set_trace_layout(TRACE_LAYOUT_DATADOG);

        let log = do_transform(trace).await.unwrap();
        assert_eq!(log.namespace(), LogNamespace::Legacy);
        assert_eq!(log.metadata().trace_layout(), None);
        assert_eq!(
            log.get(vrl::event_path!("host")),
            Some(&"a_hostname".into())
        );
    }
}
