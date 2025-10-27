use std::collections::BTreeMap;

use vrl::prelude::{expression::Expr, *};

use crate::common::{metric_into_vrl, metrics_vrl_typedef, Error, MetricsStorage};

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
        &[Parameter {
            keyword: "key",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Get the datadog api key",
            source: r#"get_vector_metric("utilization")"#,
            result: Ok("secret value"),
        }]
    }

    fn compile(
        &self,
        _state: &TypeState,
        ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let metrics = ctx
            .get_external_context::<MetricsStorage>()
            .ok_or(Box::new(Error::MetricsStorageNotLoaded) as Box<dyn DiagnosticMessage>)?
            .clone();
        let key = arguments.required("key");
        let tags = arguments.optional_object("tags")?.unwrap_or_default();
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
        let tags = self
            .tags
            .iter()
            .map(|(k, v)| {
                v.resolve(ctx).map(|v| {
                    (
                        k.clone().into(),
                        v.as_str().expect("tag must be a string").into_owned(),
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        get_metric(&self.metrics, key, tags)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(metrics_vrl_typedef()).infallible()
    }
}
