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

impl DelFn {
    #[cfg(test)]
    fn new(field: Path) -> Self {
        Self { field }
    }
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
    use crate::map;

    #[test]
    fn del() {
        let cases = vec![
            (
                // String field exists
                map!["exists": "value"],
                Ok(value!("value")),
                DelFn::new(Path::from("exists")),
            ),
            (
                // String field doesn't exist
                map!["exists": "value"],
                Ok(value!(null)),
                DelFn::new(Path::from("does_not_exist")),
            ),
            (
                // Array field exists
                map!["exists": value!([1, 2, 3])],
                Ok(value!([1, 2, 3])),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Null field exists
                map!["exists": value!(null)],
                Ok(value!(null)),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Map field exists
                map!["exists": map!["foo": "bar"]],
                Ok(value!(map!["foo": "bar"])),
                DelFn::new(Path::from("exists")),
            ),
            (
                // Integer field exists
                map!["exists": 127],
                Ok(value!(127)),
                DelFn::new(Path::from("exists")),
            )
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
