use std::collections::BTreeMap;
use vrl::prelude::expression::Expr;
use vrl::value;

use vrl::prelude::*;

use crate::common::{Error, MetricsStorage};

fn aggregate_metrics(
    metrics_storage: &MetricsStorage,
    function: &Bytes,
    key: Value,
    tags: BTreeMap<String, String>,
) -> std::result::Result<Value, ExpressionError> {
    let key_str = key.as_str().expect("argument must be a string");
    let metrics = metrics_storage.find_metrics(&key_str, tags);

    let metric_values = metrics.into_iter().filter_map(|m| match m.value() {
        vector_core::event::MetricValue::Counter { value }
        | vector_core::event::MetricValue::Gauge { value } => NotNan::new(*value).ok(),
        _ => None,
    });

    Ok(match function.as_ref() {
        b"sum" => metric_values.sum::<NotNan<f64>>().into(),
        b"avg" => {
            let len = metric_values.clone().collect::<Vec<_>>().len();
            (metric_values.sum::<NotNan<f64>>() / len as f64).into()
        }
        b"max" => metric_values.max().map(Into::into).unwrap_or(Value::Null),
        b"min" => metric_values.min().map(Into::into).unwrap_or(Value::Null),
        _ => unreachable!(),
    })
}

#[derive(Clone, Copy, Debug)]
pub struct AggregateVectorMetrics;

fn aggregation_functions() -> Vec<Value> {
    vec![value!("sum"), value!("avg"), value!("min"), value!("max")]
}

impl Function for AggregateVectorMetrics {
    fn identifier(&self) -> &'static str {
        "aggregate_vector_metrics"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "function",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "tags",
                kind: kind::OBJECT,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            example! {
                title: "Sum vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("sum", "utilization")"#,
                result: Ok("0.0"),
            },
            example! {
                title: "Sum vector internal metrics matching the name and tags",
                source: r#"aggregate_vector_metrics("sum", "utilization", tags: {"component_id": "test"})"#,
                result: Ok("0.0"),
            },
            example! {
                title: "Average of vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("avg", "utilization")"#,
                result: Ok("0.0"),
            },
            example! {
                title: "Max of vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("max", "utilization")"#,
                result: Ok("0.0"),
            },
            example! {
                title: "Min of vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("max", "utilization")"#,
                result: Ok("0.0"),
            },
        ]
    }

    fn compile(
        &self,
        state: &TypeState,
        ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let metrics = ctx
            .get_external_context::<MetricsStorage>()
            .ok_or(Box::new(Error::MetricsStorageNotLoaded) as Box<dyn DiagnosticMessage>)?
            .clone();
        let function = arguments
            .required_enum("function", &aggregation_functions(), state)?
            .try_bytes()
            .expect("aggregation function not bytes");
        let key = arguments.required("key");
        let tags = arguments.optional_object("tags")?.unwrap_or_default();

        for v in tags.values() {
            if *v.type_def(state).kind() != Kind::bytes() {
                return Err(Box::new(vrl::compiler::function::Error::InvalidArgument {
                    keyword: "tags.value",
                    value: v.resolve_constant(state).unwrap_or(Value::Null),
                    error: "Tag values must be strings",
                }));
            }
        }
        Ok(AggregateVectorMetricsFn {
            metrics,
            function,
            key,
            tags,
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
struct AggregateVectorMetricsFn {
    metrics: MetricsStorage,
    function: Bytes,
    key: Box<dyn Expression>,
    tags: BTreeMap<KeyString, Expr>,
}

impl FunctionExpression for AggregateVectorMetricsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        let tags = self
            .tags
            .iter()
            .map(|(k, v)| {
                v.resolve(ctx).and_then(|v| {
                    Ok((
                        k.clone().into(),
                        v.as_str().ok_or("Tag must be a string")?.into_owned(),
                    ))
                })
            })
            .collect::<Result<_, _>>()?;
        aggregate_metrics(&self.metrics, &self.function, key, tags)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::float().or_null().infallible()
    }
}
