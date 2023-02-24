use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::{diagnostic::Note, prelude::*};

fn assert(condition: Value, message: Option<Value>, format: Option<String>) -> Resolved {
    match condition.try_boolean()? {
        true => Ok(true.into()),
        false => {
            if let Some(message) = message {
                let message = message.try_bytes_utf8_lossy()?.into_owned();
                Err(ExpressionError::Error {
                    message: message.clone(),
                    labels: vec![],
                    notes: vec![Note::UserErrorMessage(message)],
                })
            } else {
                let message = match format {
                    Some(string) => format!("assertion failed: {string}"),
                    None => "assertion failed".to_owned(),
                };
                Err(ExpressionError::from(message))
            }
        }
    }
}

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
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let condition = arguments.required("condition");
        let message = arguments.optional("message");

        Ok(AssertFn { condition, message }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct AssertFn {
    condition: Box<dyn Expression>,
    message: Option<Box<dyn Expression>>,
}

impl FunctionExpression for AssertFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self.condition.resolve(ctx)?;
        let format = self.condition.format();
        let message = self.message.as_ref().map(|m| m.resolve(ctx)).transpose()?;

        assert(condition, message, format)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::boolean().fallible()
    }
}

impl fmt::Display for AssertFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("")
    }
}
