use ::value::Value;
use vrl::prelude::*;

fn array(value: Value) -> Result<Value> {
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

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        array(value)
    }
}

#[derive(Debug, Clone)]
struct ArrayFn {
    value: Box<dyn Expression>,
}

impl Expression for ArrayFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        array(self.value.resolve(ctx)?.into_owned()).map(Cow::Owned)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::array(Collection::any()))
            .restrict_array()
    }
}
