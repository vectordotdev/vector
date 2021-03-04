use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct GetHostname;

impl Function for GetHostname {
    fn identifier(&self) -> &'static str {
        "get_hostname"
    }

    fn compile(&self, _: ArgumentList) -> Compiled {
        Ok(Box::new(GetHostnameFn))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"get_hostname!() != """#,
            result: Ok("true"),
        }]
    }
}

#[derive(Debug, Clone)]
struct GetHostnameFn;

impl Expression for GetHostnameFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(hostname::get()
            .map_err(|error| format!("failed to get hostname: {}", error))?
            .to_string_lossy()
            .into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    remap::test_type_def![static_def {
        expr: |_| GetHostnameFn,
        def: TypeDef {
            fallible: true,
            kind: value::Kind::Bytes,
            ..Default::default()
        },
    }];

    #[test]
    fn get_hostname() {
        let mut state = state::Program::default();
        let mut object: Value = btreemap! {}.into();
        let value = GetHostnameFn.execute(&mut state, &mut object).unwrap();

        assert!(matches!(&value, Value::Bytes(_)));
    }
}
*/
