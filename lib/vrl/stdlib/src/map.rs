use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Map;

impl Function for Map {
    fn identifier(&self) -> &'static str {
        "map"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::OBJECT | kind::ARRAY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "adding numbers in array",
                source: r#"map([1,2,3]) -> |index, value| {  value + 1 }"#,
                result: Ok("[2,3,4]"),
            },
            Example {
                title: "array returning array",
                source: r#"map([1,2,3]) -> |index, value| {  [value]  }"#,
                result: Ok(r#"[[1],[2],[3]]"#),
            },
            Example {
                title: "index type checks as integer",
                source: r#"map([1,2,3]) -> |index, value| {  5 - index }"#,
                result: Ok(r#"[5,4,3]"#),
            },
            Example {
                title: "enumerating object",
                source: r#"map({"a" : 1, "b" : 2, "c" : 3}) -> |key, value| { [key, value + 1] }"#,
                result: Ok(r#"{"a" :2, "b" : 3, "c" :4}"#),
            },
            Example {
                title: "string array value",
                source: r#"map(["foo", "bar"]) -> |index, value| { value + "_" + to_string(index) }"#,
                result: Ok(r#"["foo_0", "bar_1"]"#),
            },
            Example {
                title: "map to array of objects",
                source: r#"map([1, 2]) -> |index, value| { { "a": value} }"#,

                result: Ok(r#"[{"a": 1}, {"a": 2}]"#),
            },
            Example {
                title: "map to array of objects",
                source: r#"map([1]) -> |index, value| { { "a": 1} }"#,

                result: Ok(r#"[{"a": 1}]"#),
            },
            Example {
                title: "accrue value outside map",
                source: r#"result = {};  map(["a", "b", "c"]) -> |index, value| { result = set!(value: result, path: [value], data: index) }; result"#,

                result: Ok(r#"{"a": 0, "b": 1, "c": 2}"#),
            },
            Example {
                title: "non array return value for object iteration does not compile",
                source: r#"map({"b": 2}) -> |index, value| { { "a": 1} }"#,

                result: Err(
                    r#"function call error for "map" at (0:45): object iteration requires returning a key/value array return value"#,
                ),
            },
            Example {
                title: "single value return array does not compile",
                source: r#"map({ "a": 1}) -> |key, value| { [key]  }"#,
                result: Err(
                    r#"function call error for "map" at (0:41): object iteration requires a two-element array return value"#,
                ),
            },
            Example {
                title: "non-byte key type does not compile",
                source: r#"map({ "a": 1}) -> |key, value| { [value, key]  }"#,
                result: Err(
                    r#"function call error for "map" at (0:48): object iteration requires the first element to be a string type"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &vrl::prelude::FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let closure = arguments.required_closure()?;

        Ok(Box::new(MapFn { value, closure }))
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
            output: closure::Output::Array {
                elements: vec![Kind::bytes(), Kind::any()],
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
            output: closure::Output::Any,
        };

        Some(closure::Definition {
            inputs: vec![object, array],
        })
    }
}

#[derive(Debug, Clone)]
struct MapFn {
    value: Box<dyn Expression>,
    closure: Closure,
}

impl Expression for MapFn {
    fn resolve(&self, ctx: &mut Context) -> Result<Value> {
        // - First focus on objects, ignore arrays
        // - A closure can resolve if it knows:
        //   - Closure variable signature/identifiers
        //   - Content of the object
        //   - Access to `ctx`
        //
        // - Resolving closure should be agnostic. It takes the values you want to assign to
        //   different closure variables, and it takes an `Fn` to apply to the data.

        let value = self.value.resolve(ctx)?;

        let mut map = |_: &Context, output: Output, result: &mut Value| -> Result<()> {
            match (output, result) {
                (Output::Object { key, value }, Value::Object(ref mut map)) => {
                    map.insert(key, value);
                }
                (Output::Array { element }, Value::Array(ref mut array)) => {
                    array.push(element);
                }
                _ => unreachable!(),
            };

            Ok(())
        };

        self.closure.resolve(ctx, value, &mut map)

        // let result = match self.value.resolve(ctx)? {
        //     Value::Object(object) => {
        //         let mut result = HashMap::default();

        //         for (key, value) in object.into_iter() {
        //             let ident = key.into();

        //             ctx.state_mut().insert_variable(ident, value);
        //             let v = self.closure.resolve(ctx)?.try_array()?;

        //             let v = closure.resolve_object(ctx, object)?;

        //             ctx.state_mut().remove_variable(&ident);

        //             result.insert(v[0], v[1]);
        //         }

        //         result.into()
        //     }
        //     Value::Array(array) => {
        //         let mut result = Vec::with_capacity(array.len());

        //         for (index, value) in array.into_iter().enumerate() {
        //             ctx.state_mut().insert_variable("index".into(), index);

        //             let v = run(index, value)?;
        //             result.push(v);
        //         }

        //         result.into()
        //     }
        //     _ => unreachable!("expected object or array"),
        // };
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::object(Collection::any()) | Kind::array(Collection::any()))
            .restrict_array()
    }
}
