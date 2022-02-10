use lookup_lib::{LookupBuf, SegmentBuf};
use vector_common::btreemap;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Set;

impl Function for Set {
    fn identifier(&self) -> &'static str {
        "set"
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
                title: "set existing field",
                source: r#"set!(value: {"foo": "bar"}, path: ["foo"], data: "baz")"#,
                result: Ok(r#"{ "foo": "baz" }"#),
            },
            Example {
                title: "nested fields",
                source: r#"set!(value: {}, path: ["foo", "bar"], data: "baz")"#,
                result: Ok(r#"{ "foo": { "bar" : "baz" } }"#),
            },
            Example {
                title: "indexing",
                source: r#"set!(value: [{ "foo": "bar" }], path: [0, "foo", "bar"], data: "baz")"#,
                result: Ok(r#"[{ "foo": { "bar": "baz" } }]"#),
            },
            Example {
                title: "nested indexing",
                source: r#"set!(value: {"foo": { "bar": [] }}, path: ["foo", "bar", 1], data: "baz")"#,
                result: Ok(r#"{ "foo": { "bar": [null, "baz"] } }"#),
            },
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = { "foo": true }
                    set!(value: ., path: ["bar"], data: "baz")
                "#},
                result: Ok(r#"{ "foo": true, "bar": "baz" }"#),
            },
            Example {
                title: "variable",
                source: indoc! {r#"
                    var = { "foo": true }
                    set!(value: var, path: ["bar"], data: "baz")
                "#},
                result: Ok(r#"{ "foo": true, "bar": "baz" }"#),
            },
            Example {
                title: "invalid indexing",
                source: r#"set!(value: [], path: ["foo"], data: "baz")"#,
                result: Ok(r#"{ "foo": "baz" }"#),
            },
            Example {
                title: "invalid segment type",
                source: r#"set!({"foo": { "bar": [92, 42] }}, ["foo", true], "baz")"#,
                result: Err(
                    r#"function call error for "set" at (0:56): path segment must be either "string" or "integer", not "boolean""#,
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

        Ok(Box::new(SetFn { value, path, data }))
    }
}

#[derive(Debug, Clone)]
pub struct SetFn {
    value: Box<dyn Expression>,
    path: Box<dyn Expression>,
    data: Box<dyn Expression>,
}

impl Expression for SetFn {
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

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let kind = self.value.type_def(state).kind();

        let td = TypeDef::new().fallible();

        match kind {
            Kind::Array => td.array::<Kind>(vec![]),
            Kind::Object => td.object::<(), Kind>(btreemap! {}),
            k if k.contains_array() && k.contains_object() => td
                .array::<Kind>(vec![])
                .add_object::<(), Kind>(btreemap! {}),
            _ => unreachable!("compiler guaranteed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        set => Set;

        array {
            args: func_args![value: value!([]), path: vec![0], data: true],
            want: Ok(vec![true]),
            tdef: TypeDef::new().array::<Kind>(vec![]).fallible(),
        }

        object {
            args: func_args![value: value!({}), path: vec!["foo"], data: true],
            want: Ok(value!({ "foo": true })),
            tdef: TypeDef::new().object::<(), Kind>(btreemap!{}).fallible(),
        }
    ];
}
