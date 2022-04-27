use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct MapValues;

impl Function for MapValues {
    fn identifier(&self) -> &'static str {
        "map_values"
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
                title: "map object values",
                source: r#"map_values({ "a": 1, "b": 2 }) -> |value| { value + 1 }"#,
                result: Ok(r#"{ "a": 2, "b": 3 }"#),
            },
            Example {
                title: "recursively map object values",
                source: r#"map_values({ "a": 1, "b": [{ "c": 2 }, { "d": 3 }], "e": { "f": 4 } }, recursive: true) -> |value| { if is_integer(value) { int!(value) + 1 } else { value } }"#,
                result: Ok(r#"{ "a": 2, "b": [{ "c": 3 }, { "d": 4 }], "e": { "f": 5 } }"#),
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

        Ok(Box::new(MapValuesFn {
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
                variables: vec![Variable {
                    kind: VariableKind::TargetInnerValue,
                }],
                output: Output::Kind(Kind::any()),
                example: Example {
                    title: "map object values",
                    source: r#"map_values({ "one" : "one", "two": "two" }) -> |value| { upcase(value) }"#,
                    result: Ok(r#"{ "one": "ONE", "two": "TWO" }"#),
                },
            }],
            is_iterator: true,
        })
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Result<Value> {
        // TODO: this work will happen in a follow-up PR
        Err("function currently unavailable in VM runtime".into())
    }
}

#[derive(Debug, Clone)]
struct MapValuesFn {
    value: Box<dyn Expression>,
    recursive: Option<Box<dyn Expression>>,
    closure: FunctionClosure,
}

impl Expression for MapValuesFn {
    fn resolve(&self, ctx: &mut Context) -> Result<Value> {
        let recursive = match &self.recursive {
            None => false,
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
        };

        let value = self.value.resolve(ctx)?;
        let mut iter = value.into_iter(recursive);

        for item in iter.by_ref() {
            let value = match item {
                IterItem::KeyValue(_, value) => value,
                IterItem::IndexValue(_, value) => value,
                IterItem::Value(value) => value,
            };

            self.closure.map_value(ctx, value)?;
        }

        Ok(iter.into())
    }

    fn type_def(&self, ctx: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let mut value = self.value.type_def(ctx);
        let closure = self.closure.type_def(ctx);

        recursive_type_def(&mut value, closure.kind().clone(), true);
        value.with_fallibility(closure.is_fallible())
    }
}

fn recursive_type_def(from: &mut Kind, to: Kind, root: bool) {
    if let Some(object) = from.as_object_mut() {
        for v in object.known_mut().values_mut() {
            recursive_type_def(v, to.clone(), false)
        }
    }

    if let Some(array) = from.as_array_mut() {
        for v in array.known_mut().values_mut() {
            recursive_type_def(v, to.clone(), false)
        }
    }

    if !root {
        *from = to;
    }
}
