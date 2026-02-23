use std::{collections::BTreeMap, sync::LazyLock};

use vector_vrl_category::Category;
use vrl::prelude::{expression::Expr, *};

use crate::common::{
    metric_into_vrl, metrics_vrl_typedef, resolve_tags, validate_tags, Error, MetricsStorage,
};

fn get_metric(
    metrics_storage: &MetricsStorage,
    key: Value,
    tags: BTreeMap<String, String>,
) -> Result<Value, ExpressionError> {
    let key_str = key.as_str().expect("argument must be a string");
    let value = match metrics_storage.get_metric(&key_str, tags) {
        Some(value) => metric_into_vrl(&value),
        None => Value::Null,
    };
    Ok(value)
}

static DEFAULT_TAGS: LazyLock<Value> = LazyLock::new(|| Value::Object(BTreeMap::new()));

static PARAMETERS: LazyLock<Vec<Parameter>> = LazyLock::new(|| {
    vec![
        Parameter::required("key", kind::BYTES, "The metric name to search."),
        Parameter::optional(
            "tags",
            kind::OBJECT,
            "Tags to filter the results on. Values in this object support wildcards ('*') to match on parts of the tag value.",
        )
        .default(&DEFAULT_TAGS),
    ]
});

#[derive(Clone, Copy, Debug)]
pub struct GetVectorMetric;

impl Function for GetVectorMetric {
    fn identifier(&self) -> &'static str {
        "get_vector_metric"
    }

    fn usage(&self) -> &'static str {
        const_str::concat!(
            "Searches internal Vector metrics by name and optionally by tags. Returns the first matching metric.\n\n",
            crate::VECTOR_METRICS_EXPLAINER
        )
    }

    fn category(&self) -> &'static str {
        Category::Metrics.as_ref()
    }

    fn return_kind(&self) -> u16 {
        kind::OBJECT | kind::NULL
    }

    fn parameters(&self) -> &'static [Parameter] {
        &PARAMETERS
    }

    fn examples(&self) -> &'static [Example] {
        &[
            example! {
                title: "Get a vector internal metric matching the name",
                source: r#"get_vector_metric("utilization")"#,
                result: Ok(
                    indoc! { r#"{"name": "utilization", "tags": {"component_id": ["test"]}, "type": "gauge", "kind": "absolute", "value": 0.5}"# },
                ),
            },
            example! {
                title: "Get a vector internal metric matching the name and tags",
                source: r#"get_vector_metric("utilization", tags: {"component_id": "test"})"#,
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
