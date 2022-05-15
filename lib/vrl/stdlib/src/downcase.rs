use ::value::Value;
use bytes::{BufMut, BytesMut};
use vrl::prelude::*;

use std::fmt::Write;

fn downcase(bytes: &Bytes) -> Option<Value> {
    // Bail early if no action needs to be taken.
    if Chars::new(bytes).all(|ch| ch.map(char::is_lowercase).unwrap_or(true)) {
        return None;
    }

    let mut lower = BytesMut::with_capacity(bytes.len());

    // Downcase UTF-8 chars, but keep other data as-is.
    Chars::new(bytes).for_each(|ch| match ch {
        Ok(ch) => ch.to_lowercase().for_each(|ch| {
            let _ = lower.write_char(ch);
        }),
        Err(b) => lower.put_u8(b),
    });

    Some(lower.freeze().into())
}

#[derive(Clone, Copy, Debug)]
pub struct Downcase;

impl Function for Downcase {
    fn identifier(&self) -> &'static str {
        "downcase"
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

        Ok(Box::new(DowncaseFn { value }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "downcase",
            source: r#"downcase("FOO 2 BAR")"#,
            result: Ok("foo 2 bar"),
        }]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value").try_bytes()?;
        Ok(downcase(&value).unwrap_or_else(|| Value::from(value)))
    }
}

#[derive(Debug, Clone)]
struct DowncaseFn {
    value: Box<dyn Expression>,
}

impl Expression for DowncaseFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?.try_bytes()?;
        Ok(downcase(&value)
            .map(Cow::Owned)
            .unwrap_or_else(|| Cow::Owned(value.into())))
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        downcase => Downcase;

        simple {
            args: func_args![value: "FOO 2 bar"],
            want: Ok(value!("foo 2 bar")),
            tdef: TypeDef::bytes(),
        }
    ];
}
