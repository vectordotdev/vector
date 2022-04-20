use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ForEach;

impl Function for ForEach {
    fn identifier(&self) -> &'static str {
        "for_each"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::OBJECT | kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "recursive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "iterate object",
                source: r#"count = 0; for_each({ "a": 1, "b": 2 }) -> |_key, value| { count = count + int!(value) }; count"#,
                result: Ok("3"),
            },
            Example {
                title: "recursively iterate object",
                source: r#"count = 0; for_each({ "a": 1, "b": { "c": 2, "d": 3 } }, recursive: true) -> |_, value| { count = count + (int(value) ?? 0) }; count"#,
                result: Ok("6"),
            },
            Example {
                title: "iterate array",
                source: r#"count = 0; for_each([1,2,3]) -> |index, value| { count = count + index + int!(value) }; count"#,
                result: Ok("9"),
            },
            Example {
                title: "recursively iterate array",
                source: r#"count = 0; for_each([1,2,[3,4]], recursive: true) -> |index, value| { count = count + index + (int(value) ?? 0) }; count"#,
                result: Ok("14"),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let recursive = arguments.optional("recursive");
        let closure = arguments.required_closure()?;

        Ok(Box::new(ForEachFn {
            value,
            closure,
            recursive,
        }))
    }

    fn closure(&self) -> Option<closure::Definition> {
        let object = closure::Input {
            parameter_keyword: "value",
            kind: Kind::object(Collection::any()),
            variables: vec![
                closure::Variable {
                    kind: Kind::bytes(),
                },
                closure::Variable { kind: Kind::any() },
            ],
            output: closure::Output::Kind(Kind::any()),
            example: Example {
                title: "iterate object",
                source: r#"for_each({ "one" : 1, "two": 2 }) -> |key, value| { .foo = to_int!(.foo) + int!(value) }"#,
                result: Ok("null"),
            },
        };

        let array = closure::Input {
            parameter_keyword: "value",
            kind: Kind::array(Collection::any()),
            variables: vec![
                closure::Variable {
                    kind: Kind::integer(),
                },
                closure::Variable { kind: Kind::any() },
            ],
            output: closure::Output::Kind(Kind::any()),
            example: Example {
                title: "iterate array",
                source: r#"for_each([1, 2]) -> |index, value| { .foo = to_int!(.foo) + index + int!(value) }"#,
                result: Ok("null"),
            },
        };

        let object_or_array = closure::Input {
            parameter_keyword: "value",
            kind: Kind::object(Collection::any()).or_array(Collection::any()),
            variables: vec![
                closure::Variable {
                    kind: Kind::bytes().or_integer(),
                },
                closure::Variable { kind: Kind::any() },
            ],
            output: closure::Output::Kind(Kind::any()),
            example: Example {
                title: "iterate object or array",
                source: r#"for_each([1, 2]) -> |key_or_index, value| { .foo = to_int!(.foo) + key_or_index + int!(value) }"#,
                result: Ok("null"),
            },
        };

        Some(closure::Definition {
            inputs: vec![object, array, object_or_array],
        })
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Result<Value> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct ForEachFn {
    value: Box<dyn Expression>,
    recursive: Option<Box<dyn Expression>>,
    closure: FunctionClosure,
}

impl Expression for ForEachFn {
    fn resolve(&self, ctx: &mut Context) -> Result<Value> {
        let recursive = match &self.recursive {
            None => false,
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
        };

        let value = self.value.resolve(ctx)?;
        let top_level_kind = value.kind();

        let mut iter = value.into_iter(recursive);

        for item in iter.by_ref() {
            match item {
                IterItem::KeyValue(key, value) if top_level_kind.is_object() => {
                    self.closure.run_key_value(ctx, key, value)?
                }

                IterItem::IndexValue(index, value) if top_level_kind.is_array() => {
                    self.closure.run_index_value(ctx, index, value)?
                }

                _ => {}
            };
        }

        Ok(Value::Null)
    }

    fn type_def(&self, ctx: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let fallible = self.closure.type_def(ctx).is_fallible();

        TypeDef::null().with_fallibility(fallible)
    }
}
