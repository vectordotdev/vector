use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct GetEnvVar;

impl Function for GetEnvVar {
    fn identifier(&self) -> &'static str {
        "get_env_var"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "name",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let name = arguments.required("name")?.boxed();

        Ok(Box::new(GetEnvVarFn { name }))
    }
}

#[derive(Debug, Clone)]
struct GetEnvVarFn {
    name: Box<dyn Expression>,
}

impl Expression for GetEnvVarFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.name.execute(state, object)?.try_bytes()?;
        let name = String::from_utf8_lossy(&bytes);

        let value = std::env::var(name.as_ref()).map_err(|e| e.to_string())?;
        Ok(value.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.name
            .type_def(state)
            .into_fallible(true)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    remap::test_type_def![
        value_string {
            expr: |_| GetEnvVarFn { name: Literal::from("foo").boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }

        fallible_expression {
            expr: |_| GetEnvVarFn { name: Literal::from(10).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn get_env_var() {
        let mut state = state::Program::default();
        let func = GetEnvVarFn {
            name: Box::new(Path::from("foo")),
        };
        std::env::set_var("VAR2", "var");

        let cases = vec![
            (btreemap! { "foo" => "VAR1" }, Err(())),
            (btreemap! { "foo" => "VAR2" }, Ok("var".into())),
            (btreemap! { "foo" => "=" }, Err(())),
            (btreemap! { "foo" => "" }, Err(())),
            (btreemap! { "foo" => "a=b" }, Err(())),
        ];

        for (object, expected) in cases {
            let mut object: Value = object.into();
            let got = func.execute(&mut state, &mut object).map_err(|_| ());
            assert_eq!(got, expected);
        }
    }
}
