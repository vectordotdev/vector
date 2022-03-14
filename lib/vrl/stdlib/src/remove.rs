use lookup_lib::{LookupBuf, SegmentBuf};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Remove;

impl Function for Remove {
    fn identifier(&self) -> &'static str {
        "remove"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::OBJECT | kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "path",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "compact",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "remove existing field",
                source: r#"remove!(value: {"foo": "bar"}, path: ["foo"])"#,
                result: Ok("{}"),
            },
            Example {
                title: "remove unknown field",
                source: r#"remove!(value: {"foo": "bar"}, path: ["baz"])"#,
                result: Ok(r#"{ "foo": "bar" }"#),
            },
            Example {
                title: "nested path",
                source: r#"remove!(value: {"foo": { "bar": true }}, path: ["foo", "bar"])"#,
                result: Ok(r#"{ "foo": {} }"#),
            },
            Example {
                title: "compact object",
                source: r#"remove!(value: {"foo": { "bar": true }}, path: ["foo", "bar"], compact: true)"#,
                result: Ok(r#"{}"#),
            },
            Example {
                title: "indexing",
                source: r#"remove!(value: [92, 42], path: [0])"#,
                result: Ok("[42]"),
            },
            Example {
                title: "nested indexing",
                source: r#"remove!(value: {"foo": { "bar": [92, 42] }}, path: ["foo", "bar", 1])"#,
                result: Ok(r#"{ "foo": { "bar": [92] } }"#),
            },
            Example {
                title: "compact array",
                source: r#"remove!(value: {"foo": [42], "bar": true }, path: ["foo", 0], compact: true)"#,
                result: Ok(r#"{ "bar": true }"#),
            },
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = { "foo": true }
                    remove!(value: ., path: ["foo"])
                "#},
                result: Ok("{}"),
            },
            Example {
                title: "variable",
                source: indoc! {r#"
                    var = { "foo": true }
                    remove!(value: var, path: ["foo"])
                "#},
                result: Ok("{}"),
            },
            Example {
                title: "missing index",
                source: r#"remove!(value: {"foo": { "bar": [92, 42] }}, path: ["foo", "bar", 1, -1])"#,
                result: Ok(r#"{ "foo": { "bar": [92, 42] } }"#),
            },
            Example {
                title: "invalid indexing",
                source: r#"remove!(value: [42], path: ["foo"])"#,
                result: Ok("[42]"),
            },
            Example {
                title: "invalid segment type",
                source: r#"remove!(value: {"foo": { "bar": [92, 42] }}, path: ["foo", true])"#,
                result: Err(
                    r#"function call error for "remove" at (0:65): path segment must be either string or integer, not boolean"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let path = arguments.required("path");
        let compact = arguments.optional("compact").unwrap_or(expr!(false));

        Ok(Box::new(RemoveFn {
            value,
            path,
            compact,
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RemoveFn {
    value: Box<dyn Expression>,
    path: Box<dyn Expression>,
    compact: Box<dyn Expression>,
}

impl Expression for RemoveFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let path = match self.path.resolve(ctx)? {
            Value::Array(path) => {
                let mut lookup = LookupBuf::root();

                for segment in path {
                    let segment = match segment {
                        Value::Bytes(field) => {
                            SegmentBuf::Field(String::from_utf8_lossy(&field).into_owned().into())
                        }
                        Value::Integer(index) => SegmentBuf::Index(index as isize),
                        value => {
                            return Err(format!(
                                r#"path segment must be either string or integer, not {}"#,
                                value.kind()
                            )
                            .into())
                        }
                    };

                    lookup.push_back(segment)
                }

                lookup
            }
            value => {
                return Err(value::Error::Expected {
                    got: value.kind(),
                    expected: Kind::array(Collection::any()),
                }
                .into())
            }
        };

        let compact = self.compact.resolve(ctx)?.try_boolean()?;

        let mut value = self.value.resolve(ctx)?;
        value.target_remove(&path, compact)?;

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let value_td = self.value.type_def(state);

        let mut td = TypeDef::from(Kind::empty()).fallible();

        if value_td.is_array() {
            td = td.add_array(Collection::any())
        };

        if value_td.is_object() {
            td = td.add_object(Collection::any())
        };

        td
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        remove => Remove;

        array {
            args: func_args![value: value!([42]), path: value!([0])],
            want: Ok(value!([])),
            tdef: TypeDef::array(Collection::any()).fallible(),
        }

        object {
            args: func_args![value: value!({ "foo": 42 }), path: value!(["foo"])],
            want: Ok(value!({})),
            tdef: TypeDef::object(Collection::any()).fallible(),
        }
    ];
}
