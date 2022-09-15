use ::value::Value;
use vrl::prelude::*;
use vrl::Function;
use vrl::function::Example;
use vrl::state::TypeState;
use vrl::function::FunctionCompileContext;
use vrl::function::ArgumentList;
use vrl::function::Compiled;
use vrl::Expression;

fn keys(value: Value) -> Resolved {
    let mut vec: Vec<Value> = Vec::new();
    let value_btree = value.try_object()?;
    
    for (k, _v) in value_btree {
        vec.push(Value::Bytes(Bytes::from(k.clone())))
    }
    Ok(Value::Array(vec))
}


#[derive(Debug)]
 pub struct Keys;

 impl Function for Keys {
    fn identifier(&self) -> &'static str {
        "keys"
    }
    
    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::OBJECT,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        unimplemented!();
    }
    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(KeysFn{ value }.as_expr())
    }

 }

 #[derive(Debug, Clone)]
 struct KeysFn {
    value: Box<dyn Expression>,
}

 impl FunctionExpression for KeysFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        keys(self.value.resolve(ctx)?)
    }

    fn type_def(&self, _state: &state::TypeState) -> TypeDef {

        TypeDef::array(Collection::empty().with_unknown(Kind::bytes())).infallible()
         
    }
 }