use lookup_lib::{LookupBuf, SegmentBuf};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Insert;

impl Function for Insert {
    fn identifier(&self) -> &'static str {
        "insert"
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
                keyword: "data",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "insert existing field",
                source: r#"insert!(value: {"foo": "bar"}, path: ["foo"], data: "baz")"#,
                result: Ok(r#"{ "foo": "baz" }"#),
            },
            Example {
                title: "nested fields",
                source: r#"insert!(value: {}, path: ["foo", "bar"], data: "baz")"#,
                result: Ok(r#"{ "foo": { "bar" : "baz" } }"#),
            },
            Example {
                title: "indexing",
                source: r#"insert!(value: [{ "foo": "bar" }], path: [0, "foo", "bar"], data: "baz")"#,
                result: Ok(r#"[{ "foo": { "bar": "baz" } }]"#),
            },
            Example {
                title: "nested indexing",
                source: r#"insert!(value: {"foo": { "bar": [] }}, path: ["foo", "bar", 1], data: "baz")"#,
                result: Ok(r#"{ "foo": { "bar": [null, "baz"] } }"#),
            },
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = { "foo": true }
                    insert!(value: ., path: ["bar"], data: "baz")
                "#},
                result: Ok(r#"{ "foo": true, "bar": "baz" }"#),
            },
            Example {
                title: "variable",
                source: indoc! {r#"
                    var = { "foo": true }
                    insert!(value: var, path: ["bar"], data: "baz")
                "#},
                result: Ok(r#"{ "foo": true, "bar": "baz" }"#),
            },
            Example {
                title: "invalid indexing",
                source: r#"insert!(value: [], path: ["foo"], data: "baz")"#,
                result: Ok(r#"{ "foo": "baz" }"#),
            },
            Example {
                title: "invalid segment type",
                source: r#"insert!({"foo": { "bar": [92, 42] }}, ["foo", true], "baz")"#,
                result: Err(
                    r#"function call error for "insert" at (0:59): path segment must be either "string" or "integer", not "boolean""#,
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
        let data = arguments.required("data");

        Ok(Box::new(InsertFn { value, path, data }))
    }
}

#[derive(Debug, Clone)]
pub struct InsertFn {
    value: Box<dyn Expression>,
    path: Box<dyn Expression>,
    data: Box<dyn Expression>,
}

impl Expression for InsertFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let path = match self.path.resolve(ctx)? {
            Value::Array(segments) => {
                let mut insert = LookupBuf::root();

                for segment in segments {
                    let segment = match segment {
                        Value::Bytes(path) => {
                            SegmentBuf::Field(String::from_utf8_lossy(&path).into_owned().into())
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

                    insert.push_back(segment)
                }

                insert
            }
            value => {
                return Err(value::Error::Expected {
                    got: value.kind(),
                    expected: Kind::Array | Kind::Bytes,
                }
                .into())
            }
        };

        let mut value = self.value.resolve(ctx)?;
        value.insert(&path, self.data.resolve(ctx)?)?;

        Ok(value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().unknown()
    }
}
