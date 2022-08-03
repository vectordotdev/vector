use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::{TypeDef as VrlTypeDef, *};
use vrl::state::TypeState;

fn type_def(type_def: &VrlTypeDef) -> Value {
    let mut tree = type_def.kind().canonicalize().debug_info();

    if type_def.is_fallible() {
        tree.insert("fallible".to_owned(), true.into());
    }

    tree.into()
}

/// A debug function to print the type definition of an expression at runtime.
///
/// This function is *UNDOCUMENTED* and *UNSTABLE*. It is *NOT* to be advertised
/// to users of Vector, even though it is technically useable by others.
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
        &[Example {
            title: "return type definition",
            source: r#"type_def(42)"#,
            result: Ok(r#"{ "integer": true }"#),
        }]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        // println!(
        //     "type def compile input: {:?}",
        //     state.external.target_kind().debug_info()
        // );
        let value = arguments.required("value");
        let type_def = arguments.required_type("value");
        Ok(TypeDefFn { value, type_def }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct TypeDefFn {
    value: Box<dyn Expression>,
    type_def: VrlTypeDef,
}

impl FunctionExpression for TypeDefFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        // the value isn't used, but it must be resolved to correctly resolve any side-effects
        // see: https://github.com/vectordotdev/vector/issues/13752
        let _result = self.value.resolve(ctx);
        Ok(type_def(&self.type_def.clone()))
    }

    fn type_def(&self, _state: &state::TypeState) -> VrlTypeDef {
        VrlTypeDef::any()
    }
}
