use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path};
use vrl::value::Kind;

use crate::{
    conditions::AnyCondition,
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    schema,
    transforms::Transform,
};

use super::transform::Sample;

/// Configuration for the `sample` transform.
#[configurable_component(transform(
    "sample",
    "Sample events from an event stream based on supplied criteria and at a configurable rate."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SampleConfig {
    /// The rate at which events are forwarded, expressed as `1/N`.
    ///
    /// For example, `rate = 1500` means 1 out of every 1500 events are forwarded and the rest are
    /// dropped.
    #[configurable(metadata(docs::examples = 1500))]
    pub rate: u64,

    /// The name of the field whose value is hashed to determine if the event should be
    /// sampled.
    ///
    /// Each unique value for the key creates a bucket of related events to be sampled together
    /// and the rate is applied to the buckets themselves to sample `1/N` buckets.  The overall rate
    /// of sampling may differ from the configured one if values in the field are not uniformly
    /// distributed. If left unspecified, or if the event doesn’t have `key_field`, then the
    /// event is sampled independently.
    ///
    /// This can be useful to, for example, ensure that all logs for a given transaction are
    /// sampled together, but that overall `1/N` transactions are sampled.
    #[configurable(metadata(docs::examples = "message"))]
    pub key_field: Option<String>,
    #[serde(default = "default_sample_rate_key")]
    #[configurable(metadata(docs::examples = "sample_rate"))]
    pub sample_rate_key: OptionalValuePath,

    /// A logical condition used to exclude events from sampling.
    pub exclude: Option<AnyCondition>,
}

impl GenerateConfig for SampleConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            rate: 10,
            key_field: None,
            exclude: None::<AnyCondition>,
            sample_rate_key: default_sample_rate_key(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sample")]
impl TransformConfig for SampleConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Sample::new(
            Self::NAME.to_string(),
            self.rate,
            self.key_field.clone(),
            self.exclude
                .as_ref()
                .map(|condition| condition.build(&context.enrichment_tables))
                .transpose()?,
        )))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Trace)
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::Log | DataType::Trace,
            input_definitions
                .iter()
                .map(|(output, definition)| {
                    (
                        output.clone(),
                        definition.clone().with_source_metadata(
                            SampleConfig::NAME,
                            Some(LegacyKey::Overwrite(owned_value_path!("sample_rate"))),
                            &owned_value_path!("sample_rate"),
                            Kind::bytes(),
                            None,
                        ),
                    )
                })
                .collect(),
        )]
    }
}

pub fn default_sample_rate_key() -> OptionalValuePath {
  OptionalValuePath::from(owned_value_path!("sample_rate"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::LogEvent, test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SampleConfig>();
    }

    #[tokio::test]
    async fn emits_internal_events() {
        assert_transform_compliance(async move {
            let config = SampleConfig {
                rate: 1,
                key_field: None,
                exclude: None,
                sample_rate_key: default_sample_rate_key(),
            };
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

            let log = LogEvent::from("hello world");
            tx.send(log.into()).await.unwrap();

            _ = out.recv().await;

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await
    }
}
