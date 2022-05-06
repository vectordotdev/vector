use vrl::prelude::*;

use vrl::prelude::TypeDef as VrlTypeDef;

fn type_def(type_def: &VrlTypeDef) -> Resolved {
    let mut tree = type_def.kind().debug_info();
    if type_def.is_fallible() {
        tree.insert("fallible".to_owned(), Value::Boolean(true));
    }
    Ok(Value::Object(tree))
}

#[derive(Clone, Copy, Debug)]
pub struct TypeDef;

impl Function for TypeDef {
    fn identifier(&self) -> &'static str {
        "type_def"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[]
    }

    fn compile(
        &self,
        state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(TypeDefFn {
            type_def: value.type_def((&*state.0, &*state.1)),
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Resolved {
        Ok(Value::from(
            "Unimplemented. Switch to the AST runtime to use this function.",
        ))
    }
}

#[derive(Debug, Clone)]
struct TypeDefFn {
    type_def: VrlTypeDef,
}

impl Expression for TypeDefFn {
    fn resolve(&self, _ctx: &mut Context) -> Resolved {
        type_def(&self.type_def.clone())
    }

    fn type_def(&self, _state: (&state::LocalEnv, &state::ExternalEnv)) -> VrlTypeDef {
        VrlTypeDef::any().infallible()
    }
}
