use ::value::Value;
use vrl::prelude::*;
use vrl::Function;
use vrl::function::Example;
use vrl::state::TypeState;
use vrl::function::FunctionCompileContext;
use vrl::function::ArgumentList;
use vrl::function::Compiled;
use vrl::Expression;

fn values(value: Value) -> Resolved {
    let mut vec: Vec<Value> = Vec::new();
    let value_btree = value.try_object()?;
    
    for (_k, v) in value_btree {
        vec.push(v)
    }
    Ok(Value::Array(vec))
}


#[derive(Debug)]
 pub struct Values;

 impl Function for Values {
    fn identifier(&self) -> &'static str {
        "values"
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
        Ok(ValuesFn{ value }.as_expr())
    }

 }

 #[derive(Debug, Clone)]
 struct ValuesFn {
    value: Box<dyn Expression>,
}

 impl FunctionExpression for ValuesFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        values(self.value.resolve(ctx)?)
    }

    fn type_def(&self, _state: &state::TypeState) -> TypeDef {

        TypeDef::array(Collection::empty().with_unknown(Kind::bytes())).infallible()
         
    }
 }