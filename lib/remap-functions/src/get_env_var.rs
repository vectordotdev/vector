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

impl GetEnvVarFn {
    #[cfg(test)]
    fn new(name: Box<dyn Expression>) -> Self {
        Self { name }
    }
}

impl Expression for GetEnvVarFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.name.execute(state, object)?.try_bytes()?;
        let name = String::from_utf8_lossy(&bytes);

        let value = std::env::var(name.as_ref())
            .map_err(|_| Error::Call(format!("Environment variable {:?} not found.", name)))?;
        Ok(value.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.name
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    remap::test_type_def![
        value_string {
            expr: |_| GetEnvVarFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: value::Kind::Bytes, ..Default::default() },
        }

        fallible_expression {
            expr: |_| GetEnvVarFn { value: Literal::from(10).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn get_env_var() {
        let cases = vec![
            (
                map!["foo": ""],
                Ok("".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "     "],
                Ok("".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "hi there"],
                Ok("hi there".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "           hi there        "],
                Ok("hi there".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": " \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "],
                Ok("❤❤ hi there ❤❤".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
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
