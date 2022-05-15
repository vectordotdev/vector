use ::value::Value;
use bytes::{BufMut, BytesMut};
use vrl::prelude::*;

use std::fmt::Write;

fn upcase(bytes: &Bytes) -> Option<Value> {
    // Bail early if no action needs to be taken.
    if Chars::new(bytes).all(|ch| ch.map(char::is_uppercase).unwrap_or(true)) {
        return None;
    }

    let mut upper = BytesMut::with_capacity(bytes.len());

    // Upcase UTF-8 chars, but keep other data as-is.
    Chars::new(bytes).for_each(|ch| match ch {
        Ok(ch) => ch.to_uppercase().for_each(|ch| {
            let _ = upper.write_char(ch);
        }),
        Err(b) => upper.put_u8(b),
    });

    Some(upper.freeze().into())
}

#[derive(Clone, Copy, Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "upcase",
            source: r#"upcase("foo 2 bar")"#,
            result: Ok("FOO 2 BAR"),
        }]
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(UpcaseFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value").try_bytes()?;
        Ok(upcase(&value).unwrap_or_else(|| Value::from(value)))
    }
}

#[derive(Debug, Clone)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_as_bytes()?;

        Ok(upcase(bytes).map(Cow::Owned).unwrap_or(value))
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        upcase => Upcase;

        simple {
            args: func_args![value: "FOO 2 bar"],
            want: Ok(value!("FOO 2 BAR")),
            tdef: TypeDef::bytes(),
        }
    ];
}
