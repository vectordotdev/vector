use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Downcase;

impl Function for Downcase {
    fn identifier(&self) -> &'static str {
        "downcase"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(DowncaseFn { value }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "downcase",
            source: r#"downcase("FOO 2 BAR")"#,
            result: Ok("foo 2 bar"),
        }]
    }
}

#[derive(Debug, Clone)]
struct DowncaseFn {
    value: Box<dyn Expression>,
}

impl DowncaseFn {
    /*
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
    */
}

impl Expression for DowncaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();

        Ok(String::from_utf8_lossy(&bytes).to_lowercase().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().bytes().infallible()
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn downcase() {
        let cases = vec![(
            btreemap! { "foo" => "FOO 2 bar" },
            Ok(Value::from("foo 2 bar")),
            DowncaseFn::new(Box::new(Path::from("foo"))),
        )];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }

    vrl::test_type_def![
        string {
            expr: |_| DowncaseFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: value::Kind::Bytes, ..Default::default() },
        }

        non_string {
            expr: |_| DowncaseFn { value: Literal::from(true).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Bytes, ..Default::default() },
        }
    ];
}
*/
