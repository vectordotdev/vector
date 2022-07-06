use ::value::Value;
use vrl::{diagnostic::Note, prelude::*};

fn assert_eq(left: Value, right: Value, message: Option<Value>) -> Resolved {
    if left == right {
        Ok(true.into())
    } else if let Some(message) = message {
        let message = message.try_bytes_utf8_lossy()?.into_owned();
        Err(ExpressionError::Error {
            message: message.clone(),
            labels: vec![],
            notes: vec![Note::UserErrorMessage(message)],
        })
    } else {
        Err(ExpressionError::from(format!(
            "assertion failed: {} == {}",
            left, right
        )))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AssertEq;

impl Function for AssertEq {
    fn identifier(&self) -> &'static str {
        "assert_eq"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "left",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "right",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "message",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "success",
                source: "assert_eq!(true, true)",
                result: Ok("true"),
            },
            Example {
                title: "failure",
                source: "assert_eq!(true, false)",
                result: Err(
                    r#"function call error for "assert_eq" at (0:23): assertion failed: true == false"#,
                ),
            },
            Example {
                title: "custom message",
                source: "assert_eq!(true, false, s'custom error')",
                result: Err(r#"function call error for "assert_eq" at (0:40): custom error"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let left = arguments.required("left");
        let right = arguments.required("right");
        let message = arguments.optional("message");

        Ok(Box::new(AssertEqFn {
            left,
            right,
            message,
        }))
    }
}

#[derive(Debug, Clone)]
struct AssertEqFn {
    left: Box<dyn Expression>,
    right: Box<dyn Expression>,
    message: Option<Box<dyn Expression>>,
}

impl Expression for AssertEqFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let left = self.left.resolve(ctx)?;
        let right = self.right.resolve(ctx)?;
        let message = self.message.as_ref().map(|m| m.resolve(ctx)).transpose()?;

        assert_eq(left, right, message)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        assert_eq => AssertEq;

        pass {
            args: func_args![left: "foo", right: "foo"],
            want: Ok(true),
            tdef: TypeDef::boolean().fallible(),
        }

        fail {
            args: func_args![left: "foo", right: "bar"],
            want: Err(r#"assertion failed: "foo" == "bar""#),
            tdef: TypeDef::boolean().fallible(),
        }

        message {
            args: func_args![left: "foo", right: "bar", message: "failure!"],
            want: Err("failure!"),
            tdef: TypeDef::boolean().fallible(),
        }
    ];
}
