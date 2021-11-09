use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Truncate;

impl Function for Truncate {
    fn identifier(&self) -> &'static str {
        "truncate"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "limit",
                kind: kind::INTEGER,
                required: true,
            },
            Parameter {
                keyword: "ellipsis",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "truncate",
                source: r#"truncate("foobar", 3)"#,
                result: Ok("foo"),
            },
            Example {
                title: "too short",
                source: r#"truncate("foo", 4)"#,
                result: Ok("foo"),
            },
            Example {
                title: "ellipsis",
                source: r#"truncate("foo", 2, true)"#,
                result: Ok("fo..."),
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
        let limit = arguments.required("limit");
        let ellipsis = arguments.optional("ellipsis").unwrap_or(expr!(false));

        Ok(Box::new(TruncateFn {
            value,
            limit,
            ellipsis,
        }))
    }
}

#[derive(Debug, Clone)]
struct TruncateFn {
    value: Box<dyn Expression>,
    limit: Box<dyn Expression>,
    ellipsis: Box<dyn Expression>,
}

impl Expression for TruncateFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let mut value = value.borrow().try_bytes_utf8_lossy()?.into_owned();

        let limit = self.limit.resolve(ctx)?.borrow().try_integer()?;
        let limit = if limit < 0 { 0 } else { limit as usize };

        let ellipsis = self.ellipsis.resolve(ctx)?.borrow().try_boolean()?;

        let pos = if let Some((pos, chr)) = value.char_indices().take(limit).last() {
            // char_indices gives us the starting position of the character at limit,
            // we want the end position.
            pos + chr.len_utf8()
        } else {
            // We have an empty string
            0
        };

        if value.len() > pos {
            value.truncate(pos);

            if ellipsis {
                value.push_str("...");
            }
        }

        Ok(SharedValue::from(Value::from(value)))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        truncate => Truncate;

        empty {
             args: func_args![value: "Super",
                              limit: 0,
             ],
             want: Ok(""),
             tdef: TypeDef::new().infallible().bytes(),
         }

        ellipsis {
            args: func_args![value: "Super",
                             limit: 0,
                             ellipsis: true
            ],
            want: Ok("..."),
            tdef: TypeDef::new().infallible().bytes(),
        }

        complete {
            args: func_args![value: "Super",
                             limit: 10
            ],
            want: Ok("Super"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        exact {
            args: func_args![value: "Super",
                             limit: 5,
                             ellipsis: true
            ],
            want: Ok("Super"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        big {
            args: func_args![value: "Supercalifragilisticexpialidocious",
                             limit: 5
            ],
            want: Ok("Super"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        big_ellipsis {
            args: func_args![value: "Supercalifragilisticexpialidocious",
                             limit: 5,
                             ellipsis: true,
            ],
            want: Ok("Super..."),
            tdef: TypeDef::new().infallible().bytes(),
        }

        unicode {
            args: func_args![value: "♔♕♖♗♘♙♚♛♜♝♞♟",
                             limit: 6,
                             ellipsis: true
            ],
            want: Ok("♔♕♖♗♘♙..."),
            tdef: TypeDef::new().infallible().bytes(),
        }
    ];
}
