use vrl::prelude::*;

use crate::common::{metric_into_vrl, metrics_vrl_typedef, Error, MetricsStorage};

fn find_metrics(
    metrics_storage: &MetricsStorage,
    key: Value,
) -> std::result::Result<Value, ExpressionError> {
    let key_str = key.as_str().expect("argument must be a string");
    Ok(Value::Array(
        metrics_storage
            .find_metrics(&key_str)
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

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "key",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Find the datadog api key",
            source: r#"find_vector_metrics("utilization")"#,
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
        Ok(FindVectorMetricsFn { metrics, key }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct FindVectorMetricsFn {
    metrics: MetricsStorage,
    key: Box<dyn Expression>,
}

impl FunctionExpression for FindVectorMetricsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        find_metrics(&self.metrics, key)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(Collection::from_unknown(
            Kind::object(metrics_vrl_typedef()),
        ))
        .infallible()
    }
}
