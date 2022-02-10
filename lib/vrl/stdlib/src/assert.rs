use vrl::{diagnostic::Note, prelude::*};

#[derive(Clone, Copy, Debug)]
pub struct Assert;

impl Function for Assert {
    fn identifier(&self) -> &'static str {
        "assert"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "condition",
                kind: kind::BOOLEAN,
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
                source: "assert!(true)",
                result: Ok("true"),
            },
            Example {
                title: "failure",
                source: "assert!(true == false)",
                result: Err(r#"function call error for "assert" at (0:22): assertion failed"#),
            },
            Example {
                title: "custom message",
                source: "assert!(false, s'custom error')",
                result: Err(r#"function call error for "assert" at (0:31): custom error"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let condition = arguments.required("condition");
        let message = arguments.optional("message");

        Ok(Box::new(AssertFn { condition, message }))
    }
}

#[derive(Debug, Clone)]
struct AssertFn {
    condition: Box<dyn Expression>,
    message: Option<Box<dyn Expression>>,
}

impl Expression for AssertFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        match self.condition.resolve(ctx)?.try_boolean()? {
            true => Ok(true.into()),
            false => {
                let message = self
                    .message
                    .as_ref()
                    .map(|m| {
                        m.resolve(ctx)
                            .and_then(|v| Ok(v.try_bytes_utf8_lossy()?.into_owned()))
                    })
                    .transpose()?;

                if let Some(message) = message {
                    Err(ExpressionError::Error {
                        message: message.clone(),
                        labels: vec![],
                        notes: vec![Note::UserErrorMessage(message)],
                    })
                } else {
                    let message = match self.condition.format() {
                        Some(string) => format!("assertion failed: {}", string),
                        None => "assertion failed".to_owned(),
                    };
                    Err(ExpressionError::from(message))
                }
            }
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.condition.type_def(state).fallible().boolean()
    }
}

impl fmt::Display for AssertFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("")
    }
}
