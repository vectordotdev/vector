use std::ptr::addr_of_mut;

use ::value::Value;
use vrl::prelude::*;

fn upcase(value: Value) -> Resolved {
    Ok(value.try_bytes_utf8_lossy()?.to_uppercase().into())
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
}

#[derive(Debug, Clone)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        upcase(value)
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        self.value.resolve_batch(ctx, selection_vector);

        for index in selection_vector {
            let index = *index;
            let resolved = addr_of_mut!(ctx.resolved_values[index]);
            unsafe { resolved.write(resolved.read().and_then(upcase)) };
        }
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
