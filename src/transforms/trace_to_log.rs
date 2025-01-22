use std::collections::BTreeSet;
use vector_lib::config::LogNamespace;
use vector_lib::lookup::owned_value_path;
use vector_lib::configurable::configurable_component;
use vrl::value::kind::Collection;
use vrl::value::Kind;

use crate::config::OutputId;
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Input, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::{Event, LogEvent},
    schema::Definition,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

/// Configuration for the `trace_to_log` transform.
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
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
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

    fn input(&self) -> Input {
        Input::trace()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, Definition)],
        global_log_namespace: LogNamespace,
    ) -> Vec<TransformOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = schema_definition(log_namespace);

        vec![TransformOutput::new(
            DataType::Log,
            input_definitions
                .iter()
                .map(|(output, _)| (output.clone(), schema_definition.clone()))
                .collect(),
        )]
    }
}

fn schema_definition(log_namespace: LogNamespace) -> Definition {
    let mut schema_definition = Definition::default_for_namespace(&BTreeSet::from([log_namespace]));
    
    match log_namespace {
        LogNamespace::Vector => {
            schema_definition = schema_definition.with_event_field(
                &owned_value_path!("timestamp"),
                Kind::bytes().or_undefined(),
                None,
            );

            schema_definition = schema_definition.with_metadata_field(
                &owned_value_path!("vector"),
                Kind::object(Collection::empty()),
                None,
            );
        }
        LogNamespace::Legacy => {
            if let Some(timestamp_key) = log_schema().timestamp_key() {
                schema_definition =
                    schema_definition.with_event_field(timestamp_key, Kind::timestamp(), None);
            }
        }
    }
    schema_definition
}

#[derive(Clone, Debug)]
pub struct TraceToLog;

impl FunctionTransform for TraceToLog {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let log = match event {
            Event::Trace(trace) => LogEvent::from(trace),
            _ => return,
        };
        output.push(Event::Log(log));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::components::assert_transform_compliance;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::config::ComponentKey;
    use crate::transforms::test::create_topology;
    use vector_lib::event::TraceEvent;

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
        let trace = TraceEvent::default();
        let mut metadata = trace.metadata().clone();
        metadata.set_source_id(Arc::new(ComponentKey::from("in")));
        metadata.set_upstream_id(Arc::new(OutputId::from("transform")));
        metadata.set_schema_definition(&Arc::new(schema_definition(LogNamespace::Legacy)));

        let log = do_transform(trace).await.unwrap();
        assert_eq!(log.metadata(), &metadata);
    }
}
