use std::collections::BTreeMap;
use vrl::prelude::expression::Expr;

use vrl::prelude::*;

use crate::common::{metric_into_vrl, metrics_vrl_typedef, Error, MetricsStorage};

fn find_metrics(
    metrics_storage: &MetricsStorage,
    key: Value,
    tags: BTreeMap<String, String>,
) -> std::result::Result<Value, ExpressionError> {
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
                title: "Find vector internal metrics matching the name",
                source: r#"find_vector_metrics("utilization")"#,
                result: Ok(
                    indoc! { r#"[{"name": "utilization", "tags": {}, "type": "gauge", "kind": "absolute", "value": 0.5}]"# },
                ),
            },
            example! {
                title: "Find vector internal metrics matching the name and tags",
                source: r#"find_vector_metrics("utilization", tags: {"component_id": "test"}})"#,
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

        for v in tags.values() {
            if *v.type_def(state).kind() != Kind::bytes() {
                return Err(Box::new(
                    vrl::compiler::function::Error::UnexpectedExpression {
                        keyword: "tags.value",
                        expected: "string",
                        expr: v.clone(),
                    },
                ));
            }
        }
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
        find_metrics(&self.metrics, key, tags)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(Collection::from_unknown(
            Kind::object(metrics_vrl_typedef()),
        ))
        .infallible()
    }
}
