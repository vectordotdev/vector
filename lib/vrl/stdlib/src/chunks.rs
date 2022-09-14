use ::value::Value;
use vrl::prelude::*;
use vrl::state::TypeState;

fn chunks(value: Value, chunk_size: Value) -> Resolved {
    let bytes = value.try_bytes()?;
    let chunk_size = chunk_size.try_integer()?;

    if chunk_size < 1 {
        return Err(r#""chunk_size" must be at least 1 byte"#.into());
    }

    if let Ok(chunk_size) = usize::try_from(chunk_size) {
        Ok(bytes.chunks(chunk_size).collect::<Vec<_>>().into())
    } else {
        Err(format!(
            r#""chunk_size" is too large: must be at most {} bytes"#,
            usize::MAX
        )
        .into())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Chunks;

impl Function for Chunks {
    fn identifier(&self) -> &'static str {
        "chunks"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "chunk_size",
                kind: kind::INTEGER,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "chunks by byte",
                source: r#"chunks("abcdefgh", 4)"#,
                result: Ok(r#"["abcd", "efgh"]"#),
            },
            Example {
                title: "chunk sizes do not respect unicode code point boundaries",
                source: r#"chunks("ab你好", 4)"#,
                result: Ok(r#"["ab�","�好"]"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let chunk_size = arguments.required("chunk_size");

        // chunk_size is converted to a usize, so if a user-supplied Value::Integer (i64) is
        // larger than the platform's usize::MAX, it could fail to convert.
        if let Some(literal) = chunk_size.as_value() {
            if let Some(integer) = literal.as_integer() {
                if integer < 1 {
                    return Err(vrl::function::Error::InvalidArgument {
                        keyword: "chunk_size",
                        value: literal,
                        error: r#""chunk_size" must be at least 1 byte"#,
                    }
                    .into());
                }

                if usize::try_from(integer).is_err() {
                    return Err(vrl::function::Error::InvalidArgument {
                        keyword: "chunk_size",
                        value: literal,
                        error: r#""chunk_size" is too large"#,
                    }
                    .into());
                }
            }
        }

        Ok(ChunksFn { value, chunk_size }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ChunksFn {
    value: Box<dyn Expression>,
    chunk_size: Box<dyn Expression>,
}

impl FunctionExpression for ChunksFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let chunk_size = self.chunk_size.resolve(ctx)?;

        chunks(value, chunk_size)
    }

    fn type_def(&self, _state: &TypeState) -> TypeDef {
        let not_literal = self.chunk_size.as_value().is_none();

        TypeDef::array(Collection::from_unknown(Kind::bytes())).with_fallibility(not_literal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        chunks => Chunks;

        chunks_data {
            args: func_args![value: "abcdefgh",
                             chunk_size: 4,
            ],
            want: Ok(value!(["abcd", "efgh"])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        mixed_ascii_unicode {
            args: func_args![value: "ab你好",
                             chunk_size: 4,
                             utf8: false
            ],
            want: Ok(value!([b"ab\xe4\xbd", b"\xa0\xe5\xa5\xbd"])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }
    ];
}
