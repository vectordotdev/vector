use std::collections::BTreeMap;

use vrl::prelude::{expression::Expr, *};

use crate::common::{
    metric_into_vrl, metrics_vrl_typedef, resolve_tags, validate_tags, Error, MetricsStorage,
};

fn get_metric(
    metrics_storage: &MetricsStorage,
    key: Value,
    tags: BTreeMap<String, String>,
) -> std::result::Result<Value, ExpressionError> {
    let key_str = key.as_str().expect("argument must be a string");
    let value = match metrics_storage.get_metric(&key_str, tags) {
        Some(value) => metric_into_vrl(&value),
        None => Value::Null,
    };
    Ok(value)
}

#[derive(Clone, Copy, Debug)]
pub struct GetVectorMetric;

impl Function for GetVectorMetric {
    fn identifier(&self) -> &'static str {
        "get_vector_metric"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
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
                title: "Get a vector internal metric matching the name",
                source: r#"get_vector_metrics("utilization")"#,
                result: Ok(
                    indoc! { r#"{"name": "utilization", "tags": {}, "type": "gauge", "kind": "absolute", "value": 0.5}"# },
                ),
            },
            example! {
                title: "Get a vector internal metric matching the name and tags",
                source: r#"get_vector_metrics("utilization", tags: {"component_id": "test"}})"#,
                result: Ok(
                    indoc! { r#"{"name": "utilization", "tags": {"component_id": ["test"]}, "type": "gauge", "kind": "absolute", "value": 0.5}"# },
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

        Ok(GetVectorMetricFn { metrics, key, tags }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct GetVectorMetricFn {
    metrics: MetricsStorage,
    key: Box<dyn Expression>,
    tags: BTreeMap<KeyString, Expr>,
}

impl FunctionExpression for GetVectorMetricFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        get_metric(&self.metrics, key, resolve_tags(ctx, &self.tags)?)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(metrics_vrl_typedef())
            .or_null()
            .infallible()
    }
}
