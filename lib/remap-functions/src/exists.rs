use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Exists;

impl Function for Exists {
    fn identifier(&self) -> &'static str {
        "exists"
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

        Ok(Box::new(ExistsFn { field }))
    }
}

#[derive(Debug, Clone)]
pub struct ExistsFn {
    field: Path,
}

impl ExistsFn {
    #[cfg(test)]
    fn new(field: Path) -> Self {
        Self { field }
    }
}

impl Expression for ExistsFn {
    fn execute(&self, _: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let find = object.get(self.field.as_ref());
        Ok(Value::from(!(find.is_err() || find.unwrap().is_none())))
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
    use shared::btreemap;

    #[test]
    fn exists() {
        let cases = vec![
            (
                btreemap! {},
                Ok(false.into()),
                ExistsFn::new(Path::from("foo")),
            ),
            (
                btreemap! { "foo" => 42 },
                Ok(true.into()),
                ExistsFn::new(Path::from("foo")),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object = Value::Map(object);
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
