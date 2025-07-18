use std::collections::BTreeSet;

use async_trait::async_trait;
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::{DataType, Input, TransformOutput},
    event::{
        metric::{MetricData, Sample},
        Event, MetricValue,
    },
    schema,
    transform::{FunctionTransform, OutputBuffer, Transform},
};
use vrl::value::Value;

use crate::config::{OutputId, TransformConfig, TransformContext};

/// Configuration for the `test_basic` transform.
#[configurable_component(transform("test_basic", "Test (basic)"))]
#[derive(Clone, Debug, Default)]
pub struct BasicTransformConfig {
    /// Suffix to add to the message of any log event.
    suffix: String,

    /// Amount to increase any metric by.
    increase: f64,
}

impl_generate_config_from_default!(BasicTransformConfig);

impl BasicTransformConfig {
    pub const fn new(suffix: String, increase: f64) -> Self {
        Self { suffix, increase }
    }
}

#[async_trait]
#[typetag::serde(name = "test_basic")]
impl TransformConfig for BasicTransformConfig {
    async fn build(&self, _globals: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(BasicTransform {
            suffix: self.suffix.clone(),
            increase: self.increase,
        }))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::all_bits(),
            definitions
                .iter()
                .map(|(output, definition)| (output.clone(), definition.clone()))
                .collect(),
        )]
    }
}

#[derive(Clone, Debug)]
struct BasicTransform {
    suffix: String,
    increase: f64,
}

impl FunctionTransform for BasicTransform {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        match &mut event {
            Event::Log(log) => {
                if let Some(message_key) = crate::config::log_schema().message_key_target_path() {
                    let mut v = log.get(message_key).unwrap().to_string_lossy().into_owned();
                    v.push_str(&self.suffix);
                    log.insert(message_key, Value::from(v));
                }
            }
            Event::Metric(metric) => {
                let increment = match metric.value() {
                    MetricValue::Counter { .. } => Some(MetricValue::Counter {
                        value: self.increase,
                    }),
                    MetricValue::Gauge { .. } => Some(MetricValue::Gauge {
                        value: self.increase,
                    }),
                    MetricValue::Distribution { statistic, .. } => {
                        Some(MetricValue::Distribution {
                            samples: vec![Sample {
                                value: self.increase,
                                rate: 1,
                            }],
                            statistic: *statistic,
                        })
                    }
                    MetricValue::AggregatedHistogram { .. } => None,
                    MetricValue::AggregatedSummary { .. } => None,
                    MetricValue::Sketch { .. } => None,
                    MetricValue::Set { .. } => {
                        let mut values = BTreeSet::new();
                        values.insert(self.suffix.clone());
                        Some(MetricValue::Set { values })
                    }
                };
                if let Some(increment) = increment {
                    assert!(metric.add(&MetricData {
                        kind: metric.kind(),
                        time: metric.time(),
                        value: increment,
                    }));
                }
            }
            Event::Trace(trace) => {
                if let Some(message_key) = crate::config::log_schema().message_key_target_path() {
                    let mut v = trace
                        .get(message_key)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    v.push_str(&self.suffix);
                    trace.insert(message_key, Value::from(v));
                }
            }
        };
        output.push(event);
    }
}
