use snafu::Snafu;
use vector_lib::{
    config::LegacyKey,
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalValuePath, owned_value_path},
};
use vrl::value::Kind;

use super::transform::{DynamicSampleFields, Sample, SampleMode};
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

#[derive(Debug, Snafu)]
pub enum SampleError {
    // Errors from `determine_sample_mode`
    #[snafu(display(
        "Only positive, non-zero numbers are allowed values for `ratio`, value: {ratio}"
    ))]
    InvalidRatio { ratio: f64 },

    #[snafu(display("Only non-zero numbers are allowed values for `rate`"))]
    InvalidRate,

    #[snafu(display("Only one value can be provided for either 'rate' or 'ratio', but not both"))]
    InvalidStaticConfiguration,

    #[snafu(display(
        "Only one value can be provided for either 'ratio_field' or 'rate_field', but not both"
    ))]
    InvalidDynamicConfiguration,

    #[snafu(display(
        "Exactly one value must be provided for either 'rate' or 'ratio' to configure static sampling"
    ))]
    MissingStaticConfiguration,

    #[snafu(display(
        "'key_field' cannot be combined with 'ratio_field' or 'rate_field' because dynamic values can vary per event and break key-based coherence"
    ))]
    InvalidKeyFieldDynamicCombination,
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

    /// The event field whose numeric value is used as the sampling ratio on a per-event basis.
    ///
    /// The value must be in `(0, 1]` to be considered valid. If the field is missing or invalid,
    /// static sampling settings (`rate` or `ratio`) are used as a fallback.
    /// This option cannot be used together with `rate_field`.
    #[configurable(metadata(docs::examples = "sample_rate"))]
    pub ratio_field: Option<String>,

    /// The event field whose integer value is used as the sampling rate on a per-event basis, expressed as `1/N`.
    ///
    /// The value must be a positive integer to be considered valid. If the field is missing or invalid,
    /// static sampling settings (`rate` or `ratio`) are used as a fallback.
    /// This option cannot be used together with `ratio_field`.
    #[configurable(metadata(docs::examples = "sample_rate_n"))]
    pub rate_field: Option<String>,

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
    ///
    /// This option cannot be combined with `ratio_field` or `rate_field`.
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
        if self.ratio_field.is_some() && self.rate_field.is_some() {
            return Err(SampleError::InvalidDynamicConfiguration);
        }

        if self.key_field.is_some() && (self.ratio_field.is_some() || self.rate_field.is_some()) {
            return Err(SampleError::InvalidKeyFieldDynamicCombination);
        }

        if self.rate.is_some() && self.ratio.is_some() {
            return Err(SampleError::InvalidStaticConfiguration);
        }

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
            (None, None) => Err(SampleError::MissingStaticConfiguration),
            _ => Err(SampleError::InvalidStaticConfiguration),
        }
    }
}

impl GenerateConfig for SampleConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            rate: None,
            ratio: Some(0.1),
            ratio_field: None,
            rate_field: None,
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
        let sample_mode = self.sample_rate()?;
        let exclude = self
            .exclude
            .as_ref()
            .map(|condition| condition.build(&context.enrichment_tables, &context.metrics_storage))
            .transpose()?;

        let sample = if self.ratio_field.is_some() || self.rate_field.is_some() {
            Sample::new_with_dynamic(
                Self::NAME.to_string(),
                sample_mode,
                DynamicSampleFields {
                    ratio_field: self.ratio_field.clone(),
                    rate_field: self.rate_field.clone(),
                },
                self.group_by.clone(),
                exclude,
                self.sample_rate_key.clone(),
            )
        } else {
            Sample::new(
                Self::NAME.to_string(),
                sample_mode,
                self.key_field.clone(),
                self.group_by.clone(),
                exclude,
                self.sample_rate_key.clone(),
            )
        };

        Ok(Transform::function(sample))
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
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
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
    use crate::{
        config::TransformConfig,
        transforms::sample::config::{SampleConfig, SampleError},
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SampleConfig>();
    }

    #[test]
    fn rejects_dynamic_ratio_only_configuration() {
        let config = SampleConfig {
            rate: None,
            ratio: None,
            ratio_field: Some("sample_rate".to_string()),
            rate_field: None,
            key_field: None,
            sample_rate_key: super::default_sample_rate_key(),
            group_by: None,
            exclude: None,
        };

        let err = config.sample_rate().unwrap_err();
        assert!(matches!(err, SampleError::MissingStaticConfiguration));
    }

    #[test]
    fn rejects_dynamic_rate_only_configuration() {
        let config = SampleConfig {
            rate: None,
            ratio: None,
            ratio_field: None,
            rate_field: Some("sample_rate_n".to_string()),
            key_field: None,
            sample_rate_key: super::default_sample_rate_key(),
            group_by: None,
            exclude: None,
        };

        let err = config.sample_rate().unwrap_err();
        assert!(matches!(err, SampleError::MissingStaticConfiguration));
    }

    #[test]
    fn validates_static_with_dynamic_configuration() {
        let config = SampleConfig {
            rate: Some(10),
            ratio: None,
            ratio_field: None,
            rate_field: Some("sample_rate_n".to_string()),
            key_field: None,
            sample_rate_key: super::default_sample_rate_key(),
            group_by: None,
            exclude: None,
        };

        assert!(config.validate(&crate::schema::Definition::any()).is_ok());
    }

    #[test]
    fn rejects_both_dynamic_fields_configuration() {
        let config = SampleConfig {
            rate: Some(10),
            ratio: None,
            ratio_field: Some("sample_rate".to_string()),
            rate_field: Some("sample_rate_n".to_string()),
            key_field: None,
            sample_rate_key: super::default_sample_rate_key(),
            group_by: None,
            exclude: None,
        };

        let err = config.sample_rate().unwrap_err();
        assert!(matches!(err, SampleError::InvalidDynamicConfiguration));
    }

    #[test]
    fn rejects_key_field_with_dynamic_configuration() {
        let config = SampleConfig {
            rate: Some(10),
            ratio: None,
            ratio_field: Some("sample_ratio".to_string()),
            rate_field: None,
            key_field: Some("trace_id".to_string()),
            sample_rate_key: super::default_sample_rate_key(),
            group_by: None,
            exclude: None,
        };

        let err = config.sample_rate().unwrap_err();
        assert!(matches!(
            err,
            SampleError::InvalidKeyFieldDynamicCombination
        ));
    }
}
