use std::collections::BTreeMap;
use vrl::prelude::expression::Expr;

use vrl::prelude::*;

use crate::common::{
    metric_into_vrl, metrics_vrl_typedef, resolve_tags, validate_tags, Error, MetricsStorage,
};

fn find_metrics(
    metrics_storage: &MetricsStorage,
    key: Value,
    tags: BTreeMap<String, String>,
) -> Result<Value, ExpressionError> {
    let key_str = key.as_str().expect("argument must be a string");
    Ok(Value::Array(
        metrics_storage
            .find_metrics(&key_str, tags)
            .iter()
            .map(metric_into_vrl)
            .collect(),
    ))
}

#[derive(Clone, Copy, Debug)]
pub struct FindVectorMetrics;

impl Function for FindVectorMetrics {
    fn identifier(&self) -> &'static str {
        "find_vector_metrics"
    }

    fn usage(&self) -> &'static str {
        const_str::concat!(
            "Searches internal Vector metrics by name and optionally by tags. Returns all matching metrics.\n\n",
            crate::VECTOR_METRICS_EXPLAINER
        )
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
                description: "The metric name to search.",
            },
            Parameter {
                keyword: "tags",
                kind: kind::OBJECT,
                required: false,
                description: "Tags to filter the results on. Values in this object support wildcards ('*') to match on parts of the tag value.",
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            example! {
                title: "Find vector internal metrics matching the name",
                source: r#"find_vector_metrics("utilization")"#,
                result: Ok(
                    indoc! { r#"[{"name": "utilization", "tags": {"component_id": ["test"]}, "type": "gauge", "kind": "absolute", "value": 0.5}]"# },
                ),
            },
            example! {
                title: "Find vector internal metrics matching the name and tags",
                source: r#"find_vector_metrics("utilization", tags: {"component_id": "test"})"#,
                result: Ok(
                    indoc! { r#"[{"name": "utilization", "tags": {"component_id": ["test"]}, "type": "gauge", "kind": "absolute", "value": 0.5}]"# },
                ),
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
        let key = arguments.required("key");
        let tags = arguments.optional_object("tags")?.unwrap_or_default();
        validate_tags(state, &tags)?;

        Ok(FindVectorMetricsFn { metrics, key, tags }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct FindVectorMetricsFn {
    metrics: MetricsStorage,
    key: Box<dyn Expression>,
    tags: BTreeMap<KeyString, Expr>,
}

impl FunctionExpression for FindVectorMetricsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        find_metrics(&self.metrics, key, resolve_tags(ctx, &self.tags)?)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(Collection::from_unknown(
            Kind::object(metrics_vrl_typedef()),
        ))
        .infallible()
    }
}
