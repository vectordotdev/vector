use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
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
                    Some(string) => format!("assertion failed: {}", string),
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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let condition = arguments.required("condition");
        let message = arguments.optional("message");

        Ok(Box::new(AssertFn { condition, message }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_assert",
            address: vrl_fn_assert as _,
            uses_context: false,
        })
    }
}

#[derive(Debug, Clone)]
struct AssertFn {
    condition: Box<dyn Expression>,
    message: Option<Box<dyn Expression>>,
}

impl Expression for AssertFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let condition = self.condition.resolve(ctx)?;
        let message = self.message.as_ref().map(|m| m.resolve(ctx)).transpose()?;
        let format = self.condition.format();

        assert(condition, message, format)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().fallible()
    }
}

impl fmt::Display for AssertFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("")
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_assert(condition: Value, message: Option<Value>) -> Resolved {
    assert(condition, message, None)
}
