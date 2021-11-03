use std::{cell::RefCell, rc::Rc};

use vrl::prelude::*;

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
}

#[derive(Debug, Clone)]
struct DowncaseFn {
    value: Box<dyn Expression>,
}

impl Expression for DowncaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let bytes = bytes.borrow();
        let bytes = bytes.as_bytes().unwrap();

        Ok(Rc::new(RefCell::new(
            String::from_utf8_lossy(&bytes).to_lowercase().into(),
        )))
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
            want: Ok(shared_value!("foo 2 bar")),
            tdef: TypeDef::new().bytes(),
        }
    ];
}
