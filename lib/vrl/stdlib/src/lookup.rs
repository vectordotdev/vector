use lookup_lib::{LookupBuf, SegmentBuf};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Lookup;

impl Function for Lookup {
    fn identifier(&self) -> &'static str {
        "lookup"
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
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "returns existing field",
                source: r#"lookup!(value: {"foo": "bar"}, path: ["foo"])"#,
                result: Ok(r#""bar""#),
            },
            Example {
                title: "returns null for unknown field",
                source: r#"lookup!(value: {"foo": "bar"}, path: ["baz"])"#,
                result: Ok("null"),
            },
            Example {
                title: "nested path",
                source: r#"lookup!(value: {"foo": { "bar": true }}, path: ["foo", "bar"])"#,
                result: Ok(r#"true"#),
            },
            Example {
                title: "indexing",
                source: r#"lookup!(value: [92, 42], path: [0])"#,
                result: Ok("92"),
            },
            Example {
                title: "nested indexing",
                source: r#"lookup!(value: {"foo": { "bar": [92, 42] }}, path: ["foo", "bar", 1])"#,
                result: Ok("42"),
            },
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = { "foo": true }
                    lookup!(value: ., path: ["foo"])
                "#},
                result: Ok("true"),
            },
            Example {
                title: "variable",
                source: indoc! {r#"
                    var = { "foo": true }
                    lookup!(value: var, path: ["foo"])
                "#},
                result: Ok("true"),
            },
            Example {
                title: "missing index",
                source: r#"lookup!(value: {"foo": { "bar": [92, 42] }}, path: ["foo", "bar", 1, -1])"#,
                result: Ok("null"),
            },
            Example {
                title: "invalid indexing",
                source: r#"lookup!(value: [42], path: ["foo"])"#,
                result: Ok("null"),
            },
            Example {
                title: "invalid segment type",
                source: r#"lookup!(value: {"foo": { "bar": [92, 42] }}, path: ["foo", true])"#,
                result: Err(
                    r#"function call error for "lookup" at (0:65): path segment must be either "string" or "integer", not "boolean""#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let path = arguments.required("path");

        Ok(Box::new(LookupFn { value, path }))
    }
}

#[derive(Debug, Clone)]
pub struct LookupFn {
    value: Box<dyn Expression>,
    path: Box<dyn Expression>,
}

impl Expression for LookupFn {
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
                                r#"path segment must be either "string" or "integer", not {}"#,
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
                    expected: Kind::Array,
                }
                .into())
            }
        };

        Ok(self.value.resolve(ctx)?.get(&path)?.unwrap_or(Value::Null))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().unknown()
    }
}
