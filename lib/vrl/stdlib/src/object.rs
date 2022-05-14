use ::value::Value;
use vrl::prelude::*;

#[inline]
fn check(value: &Value) -> Result<()> {
    match value {
        Value::Object(_) => Ok(()),
        _ => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::object(BTreeMap::default()),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Object;

impl Function for Object {
    fn identifier(&self) -> &'static str {
        "object"
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
                source: r#"object({"foo": "bar"})"#,
                result: Ok(r#"{"foo": "bar"}"#),
            },
            Example {
                title: "invalid",
                source: "object!(true)",
                result: Err(
                    r#"function call error for "object" at (0:13): expected object, got boolean"#,
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

        Ok(Box::new(ObjectFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        check(&value)?;

        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct ObjectFn {
    value: Box<dyn Expression>,
}

impl Expression for ObjectFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        check(&value)?;

        Ok(value)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::object(Collection::any()))
            .restrict_object()
    }
}
