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
                source: r#"count = 0; for_each({ "a": 1, "b": 2 }) -> |_key, value| { count = count + value }; count"#,
                result: Ok("3"),
            },
            Example {
                title: "recursively iterate object",
                source: r#"count = 0; for_each({ "a": 1, "b": { "c": 2, "d": 3 } }, recursive: true) -> |_, value| { count = count + (int(value) ?? 0) }; count"#,
                result: Ok("6"),
            },
            Example {
                title: "iterate array",
                source: r#"count = 0; for_each([1,2,3]) -> |index, value| { count = count + index + value }; count"#,
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
        use closure::{Definition, Input, Output, Variable, VariableKind};

        Some(Definition {
            inputs: vec![Input {
                parameter_keyword: "value",
                kind: Kind::object(Collection::any()).or_array(Collection::any()),
                variables: vec![
                    Variable {
                        kind: VariableKind::TargetInnerKey,
                    },
                    Variable {
                        kind: VariableKind::TargetInnerValue,
                    },
                ],
                output: Output::Kind(Kind::any()),
                example: Example {
                    title: "iterate array",
                    source: r#"for_each([1, 2]) -> |index, value| { .foo = to_int!(.foo) + index + value }"#,
                    result: Ok("null"),
                },
            }],
            is_iterator: true,
        })
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Result<Value> {
        // TODO: this work will happen in a follow-up PR
        Ok(Value::Null)
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
                // NOTE:
                //
                // `for_each` currently only supports recursively reading nested
                // key/value pairs if the top-level type is an object. It does
                // still recurse "through" arrays though, but the array values
                // aren't returned during iteration.
                //
                // Meaning, for this data structure:
                //
                // ```
                // { "foo": { "bar": [{ "baz": true }] } }
                // ```
                //
                // A recursive iterator would return the following items:
                //
                // ```
                // ("foo", { "bar": [{ "baz": true }] })
                // ("bar", [{ "baz": true }])
                // ("baz", true)
                // ```
                //
                // Notably missing here is the tuple:
                //
                // ```
                // (0, { "baz": true })
                // ```
                //
                // Because this returns a non-key/value pair (it returns an
                // index/value pair instead).
                //
                // Technically we can support this (easily), but it would mean
                // the first element in the tuple will almost always be marked
                // as "either a string or an integer", which would mean more
                // type coercion needs to be done by the caller, which is
                // annoying. The only case where type coercion wouldn't be
                // needed would be when iterating an object in which all
                // nested collections are known to be the same type as the
                // top-level collection, which is almost never the case (that
                // is, it's almost never the case we _know_ that this is so at
                // compile-time, not that the structure will actually be so at
                // runtime).
                //
                // We could in the future add a new `mixed: bool` option to the
                // function call, to optionally support this type of recursion,
                // at the expense of more type coercion requirements by the
                // caller.
                IterItem::KeyValue(key, value) if top_level_kind.is_object() => {
                    self.closure.run_key_value(ctx, key, value)?
                }

                // The same applies here as above, i.e. index/value pairs are
                // only returned if the top-level item is an array.
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
