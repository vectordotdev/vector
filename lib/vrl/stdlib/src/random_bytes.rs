use bytes::BytesMut;
use rand::{thread_rng, Rng, RngCore};
use std::str::Split;
use vrl::prelude::expression::Expr;
use vrl::prelude::value::Error;
use vrl::prelude::*;

const MAX_LENGTH: i64 = 1024 * 64;

fn random_bytes(length: Value) -> Resolved {
    let length = length.try_integer()?;
    if length < 0 {
        return Err(format!("Length cannot be negative").into());
    }
    if length > MAX_LENGTH {
        return Err(format!("Length is too large. Maximum is {}", MAX_LENGTH).into());
    }

    let mut output = vec![0_u8; length as usize];

    // ThreadRng is a cryptographically secure generator
    thread_rng().fill_bytes(&mut output);

    Ok(Value::Bytes(Bytes::from(output)))
}

#[derive(Clone, Copy, Debug)]
pub struct RandomBytes;

impl Function for RandomBytes {
    fn identifier(&self) -> &'static str {
        "random_bytes"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "length",
            kind: kind::INTEGER,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "get 10 random bytes",
            source: r#"length(random_bytes!(10))"#,
            result: Ok("10"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let length = arguments.required("length");

        Ok(Box::new(RandomBytesFn { length }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let length = args.required("length");
        random_bytes(length)
    }
}

#[derive(Debug, Clone)]
struct RandomBytesFn {
    length: Box<dyn Expression>,
}

impl Expression for RandomBytesFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let length = self.length.resolve(ctx)?;
        random_bytes(length)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}
