use vrl::{prelude::*, Vm};

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
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

    fn call(&self, vm: &mut Vm) {
        let stack = vm.stack_mut();
        let value = stack.pop().unwrap();
        stack.push(value.try_bytes_utf8_lossy().unwrap().to_lowercase().into())
    }
}

#[derive(Debug, Clone)]
struct DowncaseFn {
    value: Box<dyn Expression>,
}

impl Expression for DowncaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;

        Ok(String::from_utf8_lossy(&bytes).to_lowercase().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().bytes().infallible()
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
            tdef: TypeDef::new().bytes(),
        }
    ];
}
