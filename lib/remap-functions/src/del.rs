use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Del;

impl Function for Del {
    fn identifier(&self) -> &'static str {
        "del"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "field",
            accepts: |_| true,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let field = arguments.required_path("field")?;

        Ok(Box::new(DelFn { field }))
    }
}

#[derive(Debug, Clone)]
pub struct DelFn {
    field: Path,
}

impl Expression for DelFn {
    fn execute(&self, _: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        match object.remove_and_get(self.field.as_ref(), false) {
            Ok(Some(val)) => Ok(val),
            _ => Ok(Value::Null),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        use value::Kind;

        TypeDef {
            fallible: false,
            kind: Kind::Bytes
                | Kind::Integer
                | Kind::Float
                | Kind::Boolean
                | Kind::Map
                | Kind::Array
                | Kind::Timestamp
                | Kind::Regex
                | Kind::Null,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_type_def![static_type_def {
        expr: |_| DelFn {
            field: Path::from("foo"),
        },
        def: TypeDef {
            fallible: false,
            kind: value::Kind::Null,
            ..Default::default()
        },
    }];
}
