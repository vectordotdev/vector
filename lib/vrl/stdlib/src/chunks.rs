use ::value::Value;
use vrl::prelude::*;

fn chunks(value: Value, chunk_size: Value) -> Resolved {
    let chunk_size = chunk_size.try_integer()? as usize;
    let bytes = value.try_bytes()?;

    let data_length = bytes.len();
    let mut chunked_data: Vec<&[u8]> = Vec::new();

    let mut backtrack = 0;
    let mut start = 0;
    let mut end;

    if chunk_size < 4 {
        return Err(r#""chunk_size" must be greater than or equal to 4 bytes"#.into());
    }

    loop {
        let is_last_chunk = (start < data_length) && (start + chunk_size >= data_length);

        if is_last_chunk {
            chunked_data.push(&bytes[start..]);
            break;
        }

        end = start + chunk_size - backtrack;

        if is_root_utf8_byte(&bytes[end]) {
            chunked_data.push(&bytes[start..end]);
            backtrack = 0;
            start = end;
        } else {
            backtrack += 1;

            if backtrack > 3 {
                return Err("Bytes are not valid UTF-8".into());
            }
        }
    }

    Ok(chunked_data.into_iter().collect::<Vec<_>>().into())
}

fn is_root_utf8_byte(byte: &u8) -> bool {
    byte >> 6 != 0b10
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
        &[Example {
            title: "chunks by byte",
            source: r#"chunks("foobar", 1)"#,
            result: Ok("hi"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let chunk_size = arguments.required("chunk_size");

        Ok(Box::new(ChunksFn { value, chunk_size }))
    }
}

#[derive(Debug, Clone)]
struct ChunksFn {
    value: Box<dyn Expression>,
    chunk_size: Box<dyn Expression>,
}

impl Expression for ChunksFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let chunk_size = self.chunk_size.resolve(ctx)?;

        chunks(value, chunk_size)
    }

    fn type_def(&self, _state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::array(Collection::from_unknown(Kind::bytes())).infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        chunks => Chunks;

        unicode {
            args: func_args![value: "你好",
                             chunk_size: 4
            ],
            want: Ok(value!(["你", "好"])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        minimum_chunk_size {
            args: func_args![value: "",
            chunk_size: 2
            ],
            want: Err(r#""chunk_size" must be greater than or equal to 4 bytes"#),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        mixed_ascii_unicode {
            args: func_args![value: "ab你好",
                             chunk_size: 4
            ],
            want: Ok(value!(["ab", "你", "好"])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        invalid_utf8 {
            args: func_args![value: b"\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0",
                             chunk_size: 8
            ],
            want: Err("Bytes are not valid UTF-8"),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }
    ];
}
