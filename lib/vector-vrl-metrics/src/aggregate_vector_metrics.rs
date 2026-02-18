use std::collections::BTreeMap;
use std::sync::LazyLock;
use vector_vrl_category::Category;
use vrl::prelude::expression::Expr;
use vrl::prelude::function::EnumVariant;
use vrl::value;

use vrl::prelude::*;

use crate::common::resolve_tags;
use crate::common::validate_tags;
use crate::common::{Error, MetricsStorage};

static DEFAULT_TAGS: LazyLock<Value> = LazyLock::new(|| Value::Object(BTreeMap::new()));
static PARAMETERS: LazyLock<Vec<Parameter>> = LazyLock::new(|| {
    vec![
        Parameter::required("function", kind::BYTES, "The metric name to search.")
            .enum_variants(&[
                EnumVariant {
                    value: "sum",
                    description: "Sum the values of all the matched metrics.",
                },
                EnumVariant {
                    value: "avg",
                    description: "Find the average of the values of all the matched metrics.",
                },
                EnumVariant {
                    value: "max",
                    description: "Find the highest metric value of all the matched metrics.",
                },
                EnumVariant {
                    value: "min",
                    description: "Find the lowest metric value of all the matched metrics.",
                },
            ]),
        Parameter::required("key", kind::BYTES, "The metric name to aggregate."),
        Parameter::optional(
            "tags",
            kind::OBJECT,
            "Tags to filter the results on. Values in this object support wildcards ('*') to match on parts of the tag value.",
        )
        .default(&DEFAULT_TAGS),
    ]
});

fn aggregate_metrics(
    metrics_storage: &MetricsStorage,
    function: &Bytes,
    key: Value,
    tags: BTreeMap<String, String>,
) -> Result<Value, ExpressionError> {
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

    fn usage(&self) -> &'static str {
        const_str::concat!(
            "Aggregates internal Vector metrics, using one of 4 aggregation functions, filtering by name and optionally by tags. Returns the aggregated value. Only includes counter and gauge metrics.\n\n",
            crate::VECTOR_METRICS_EXPLAINER
        )
    }

    fn category(&self) -> &'static str {
        Category::Metrics.as_ref()
    }

    fn return_kind(&self) -> u16 {
        kind::FLOAT | kind::NULL
    }

    fn parameters(&self) -> &'static [Parameter] {
        &PARAMETERS
    }

    fn examples(&self) -> &'static [Example] {
        &[
            example! {
                title: "Sum vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("sum", "utilization")"#,
                result: Ok("0.5"),
            },
            example! {
                title: "Sum vector internal metrics matching the name and tags",
                source: r#"aggregate_vector_metrics("sum", "utilization", tags: {"component_id": "test"})"#,
                result: Ok("0.5"),
            },
            example! {
                title: "Average of vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("avg", "utilization")"#,
                result: Ok("0.5"),
            },
            example! {
                title: "Max of vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("max", "utilization")"#,
                result: Ok("0.5"),
            },
            example! {
                title: "Min of vector internal metrics matching the name",
                source: r#"aggregate_vector_metrics("max", "utilization")"#,
                result: Ok("0.5"),
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
        validate_tags(state, &tags)?;

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
        aggregate_metrics(
            &self.metrics,
            &self.function,
            key,
            resolve_tags(ctx, &self.tags)?,
        )
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::float().or_null().infallible()
    }
}
