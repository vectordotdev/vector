use snafu::Snafu;
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path};
use vrl::value::Kind;

use crate::{
    conditions::AnyCondition,
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    schema,
    template::Template,
    transforms::Transform,
};

use super::transform::{Sample, SampleMode};

#[derive(Debug, Snafu)]
pub enum SampleError {
    // Errors from `determine_sample_mode`
    #[snafu(display(
        "Only positive, non-zero numbers are allowed values for `ratio`, value: {ratio}"
    ))]
    InvalidRatio { ratio: f64 },

    #[snafu(display("Only non-zero numbers are allowed values for `rate`"))]
    InvalidRate,

    #[snafu(display(
        "Exactly one value must be provided for either 'rate' or 'ratio', but not both"
    ))]
    InvalidConfiguration,
}

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
    /// dropped. This differs from `ratio` which allows more precise control over the number of events
    /// retained and values greater than 1/2. It is an error to provide a value for both `rate` and `ratio`.
    #[configurable(metadata(docs::examples = 1500))]
    pub rate: Option<u64>,

    /// The rate at which events are forwarded, expressed as a percentage
    ///
    /// For example, `ratio = .13` means that 13% out of all events on the stream are forwarded and
    /// the rest are dropped. This differs from `rate` allowing the configuration of a higher
    /// precision value and also the ability to retain values of greater than 50% of all events. It is
    /// an error to provide a value for both `rate` and `ratio`.
    #[configurable(metadata(docs::examples = 0.13))]
    #[configurable(validation(range(min = 0.0, max = 1.0)))]
    pub ratio: Option<f64>,

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

    /// The event key in which the sample rate is stored. If set to an empty string, the sample rate will not be added to the event.
    #[configurable(metadata(docs::examples = "sample_rate"))]
    #[serde(default = "default_sample_rate_key")]
    pub sample_rate_key: OptionalValuePath,

    /// The value to group events into separate buckets to be sampled independently.
    ///
    /// If left unspecified, or if the event doesn't have `group_by`, then the event is not
    /// sampled separately.
    #[configurable(metadata(
        docs::examples = "{{ service }}",
        docs::examples = "{{ hostname }}-{{ service }}"
    ))]
    pub group_by: Option<Template>,

    /// A logical condition used to exclude events from sampling.
    pub exclude: Option<AnyCondition>,
}

impl SampleConfig {
    fn sample_rate(&self) -> Result<SampleMode, SampleError> {
        match (self.rate, self.ratio) {
            (None, Some(ratio)) => {
                if ratio <= 0.0 {
                    Err(SampleError::InvalidRatio { ratio })
                } else {
                    Ok(SampleMode::new_ratio(ratio))
                }
            }
            (Some(rate), None) => {
                if rate == 0 {
                    Err(SampleError::InvalidRate)
                } else {
                    Ok(SampleMode::new_rate(rate))
                }
            }
            _ => Err(SampleError::InvalidConfiguration),
        }
    }
}

impl GenerateConfig for SampleConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            rate: None,
            ratio: Some(0.1),
            key_field: None,
            group_by: None,
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
            self.sample_rate()?,
            self.key_field.clone(),
            self.group_by.clone(),
            self.exclude
                .as_ref()
                .map(|condition| condition.build(&context.enrichment_tables))
                .transpose()?,
            self.sample_rate_key.clone(),
        )))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Trace)
    }

    fn validate(&self, _: &schema::Definition) -> Result<(), Vec<String>> {
        self.sample_rate()
            .map(|_| ())
            .map_err(|e| vec![e.to_string()])
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
    use crate::transforms::sample::config::SampleConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SampleConfig>();
    }
}
