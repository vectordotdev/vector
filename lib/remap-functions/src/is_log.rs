use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsLog;

impl Function for IsLog {
    fn identifier(&self) -> &'static str {
        "is_log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn compile(&self, _arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(IsLogFn))
    }
}

#[derive(Debug, Clone)]
struct IsLogFn;

impl Expression for IsLogFn {
    fn execute(&self, _state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        Ok((object.identifier() == "log").into())
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
        expr: |_| IsLogFn,
        def: TypeDef {
            kind: value::Kind::Boolean,
            ..Default::default()
        },
    }];
}
