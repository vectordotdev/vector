use ::value::Value;
use vrl::prelude::*;

fn array(value: Value) -> Resolved {
    match value {
        v @ Value::Array(_) => Ok(v),
        v => Err(format!("expected array, got {}", v.kind()).into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Array;

impl Function for Array {
    fn identifier(&self) -> &'static str {
        "array"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: r#"array([1,2,3])"#,
                result: Ok("[1,2,3]"),
            },
            Example {
                title: "invalid",
                source: "array!(true)",
                result: Err(
                    r#"function call error for "array" at (0:12): expected array, got boolean"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ArrayFn { value }))
    }

    fn symbol(&self) -> Option<(&'static str, usize)> {
        Some(("vrl_fn_array", vrl_fn_array as _))
    }
}

#[derive(Debug, Clone)]
struct ArrayFn {
    value: Box<dyn Expression>,
}

impl Expression for ArrayFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        array(self.value.resolve(ctx)?)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::array(Collection::any()))
            .restrict_array()
    }
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn vrl_fn_array(value: &mut Value, result: &mut Resolved) {
    let value = {
        let mut moved = Value::Null;
        std::mem::swap(value, &mut moved);
        moved
    };

    *result = array(value);
}
