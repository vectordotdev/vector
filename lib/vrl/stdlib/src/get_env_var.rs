use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct GetEnvVar;

impl Function for GetEnvVar {
    fn identifier(&self) -> &'static str {
        "get_env_var"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "name",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let name = arguments.required("name");

        Ok(Box::new(GetEnvVarFn { name }))
    }
}

#[derive(Debug)]
struct GetEnvVarFn {
    name: Box<dyn Expression>,
}

impl Expression for GetEnvVarFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.name.resolve(ctx)?.try_bytes()?;
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
    use crate::map;

    vrl::test_type_def![
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
            (map!["foo": "VAR1"], Err(())),
            (map!["foo": "VAR2"], Ok("var".into())),
            (map!["foo": "="], Err(())),
            (map!["foo": ""], Err(())),
            (map!["foo": "a=b"], Err(())),
        ];

        for (object, expected) in cases {
            let mut object: Value = object.into();
            let got = func.resolve(&mut ctx).map_err(|_| ());
            assert_eq!(got, expected);
        }
    }
}
