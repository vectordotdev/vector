use ::value::Value;
use vrl::function::ArgumentList;
use vrl::function::Compiled;
use vrl::function::Example;
use vrl::function::FunctionCompileContext;
use vrl::prelude::*;
use vrl::state::TypeState;
use vrl::Expression;
use vrl::Function;

fn keys(value: Value) -> Resolved {
    let object = value.try_object()?;
    let keys = object.into_keys().map(Value::from);
    Ok(Value::Array(keys.collect()))
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
        &[
            Example {
                title: "get keys",
                source: r#"keys({"key1": "val1", "key2": "val2"})"#,
                result: Ok(r#"["key1", "key2"]"#),
            },
            Example {
                title: "get keys from a nested object",
                source: r#"keys({"key1": "val1", "key2": {"nestedkey1": "val3", "nestedkey2": "val4"}})"#,
                result: Ok(r#"["key1", "key2"]"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(KeysFn { value }.as_expr())
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
