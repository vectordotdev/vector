use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsMetric;

impl Function for IsMetric {
    fn identifier(&self) -> &'static str {
        "is_metric"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn compile(&self, _arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(IsMetricFn))
    }
}

#[derive(Debug, Clone)]
struct IsMetricFn;

impl Expression for IsMetricFn {
    fn execute(&self, _state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        Ok((object.identifier() == "metric").into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef {
            fallible: false,
            kind: value::Kind::Boolean,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_type_def![infallible {
        expr: |_| IsMetricFn,
        def: TypeDef {
            kind: value::Kind::Boolean,
            ..Default::default()
        },
    }];
}
