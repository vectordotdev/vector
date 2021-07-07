use lookup::LookupBuf;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Unnest;

impl Function for Unnest {
    fn identifier(&self) -> &'static str {
        "unnest"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "path",
            kind: kind::ARRAY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = {"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}
                    . = unnest!(.events)
                "#},
                result: Ok(
                    r#"[{"hostname": "localhost", "events": {"message": "hello"}}, {"hostname": "localhost", "events": {"message": "world"}}]"#,
                ),
            },
            Example {
                title: "variable target",
                source: indoc! {r#"
                    foo = {"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}
                    foo = unnest!(foo.events)
                "#},
                result: Ok(
                    r#"[{"hostname": "localhost", "events": {"message": "hello"}}, {"hostname": "localhost", "events": {"message": "world"}}]"#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let path = arguments.required_query("path")?;

        Ok(Box::new(UnnestFn { path }))
    }
}

#[derive(Debug, Clone)]
struct UnnestFn {
    path: expression::Query,
}

impl UnnestFn {
    #[cfg(test)]
    fn new(path: &str) -> Self {
        use std::str::FromStr;

        Self {
            path: expression::Query::new(
                expression::Target::External,
                FromStr::from_str(path).unwrap(),
            ),
        }
    }
}

impl Expression for UnnestFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let path = self.path.path();

        let value: Value;
        let target: Box<&dyn Target> = match self.path.target() {
            expression::Target::External => Box::new(ctx.target()) as Box<_>,
            expression::Target::Internal(v) => {
                let v = ctx.state().variable(v.ident()).unwrap_or(&Value::Null);
                Box::new(v as &dyn Target) as Box<_>
            }
            expression::Target::Container(expr) => {
                value = expr.resolve(ctx)?;
                Box::new(&value as &dyn Target) as Box<&dyn Target>
            }
            expression::Target::FunctionCall(expr) => {
                value = expr.resolve(ctx)?;
                Box::new(&value as &dyn Target) as Box<&dyn Target>
            }
        };

        let root = target.get(&LookupBuf::root())?.unwrap_or(Value::Null);

        let values = root
            .get_by_path(path)
            .cloned()
            .ok_or(value::Error::Expected {
                got: Kind::Null,
                expected: Kind::Array,
            })?
            .try_array()?;

        let events = values
            .into_iter()
            .map(|value| {
                let mut event = root.clone();
                event.insert_by_path(path, value);
                event
            })
            .collect::<Vec<_>>();

        Ok(Value::Array(events))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        match state.target_type_def() {
            Some(root_type_def) => root_type_def
                .clone()
                .invert_array_at_path(&self.path.path())
                .fallible(),
            None => self
                .path
                .type_def(state)
                .fallible_unless(Kind::Object)
                .restrict_array()
                .add_null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn unnest() {
        let cases = vec![
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Ok(
                    value!([{"hostname": "localhost", "events": {"message": "hello"}}, {"hostname": "localhost", "events": {"message": "world"}}]),
                ),
                UnnestFn::new("events"),
                TypeDef::new()
                    .array_mapped::<(), TypeDef>(btreemap! {
                        () => TypeDef::new().object::<&'static str, TypeDef>(btreemap! {
                            "hostname" => TypeDef::new().bytes(),
                            "events" => TypeDef::new().object::<&'static str, TypeDef>(btreemap! {
                                "message" => TypeDef::new().bytes()
                            })
                        })
                    })
                    .fallible(),
            ),
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Err(r#"expected "array", got "null""#.to_owned()),
                UnnestFn::new("unknown"),
                TypeDef::new().fallible(),
            ),
            (
                value!({"hostname": "localhost", "events": [{"message": "hello"}, {"message": "world"}]}),
                Err(r#"expected "array", got "string""#.to_owned()),
                UnnestFn::new("hostname"),
                TypeDef::new().fallible(),
            ),
        ];

        let compiler = state::Compiler::new_with_type_def(
            TypeDef::new().object::<&'static str, TypeDef>(btreemap! {
                "hostname" => TypeDef::new().bytes(),
                "events" => TypeDef::new().array_mapped::<(), TypeDef>(btreemap! {
                        () => TypeDef::new().object::<&'static str, TypeDef>(btreemap! {
                            "message" => TypeDef::new().bytes()
                        })
                }),
            }),
        );

        for (object, expected, func, expected_typedef) in cases {
            let mut object = object.clone();
            let mut runtime_state = vrl::state::Runtime::default();
            let mut ctx = Context::new(&mut object, &mut runtime_state);

            let typedef = func.type_def(&compiler);

            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, expected);
            assert_eq!(typedef, expected_typedef);
        }
    }
}
