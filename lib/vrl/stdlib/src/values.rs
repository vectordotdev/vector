use ::value::Value;
use vrl::function::ArgumentList;
use vrl::function::Compiled;
use vrl::function::Example;
use vrl::function::FunctionCompileContext;
use vrl::prelude::*;
use vrl::state::TypeState;
use vrl::Expression;
use vrl::Function;

fn values(value: Value) -> Resolved {
    let object = value.try_object()?;
    let values = object.into_values();
    Ok(Value::Array(values.collect()))
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
        &[
            Example {
                title: "get values",
                source: r#"values({"key1": "val1", "key2": "val2"})"#,
                result: Ok(r#"["val1", "val2"]"#),
            },
            Example {
                title: "get values from a nested object",
                source: r#"values({"key1": "val1", "key2": {"nestedkey1": "val3", "nestedkey2": "val4"}})"#,
                result: Ok(r#"["val1", { "nestedkey1": "val3", "nestedkey2": "val4" }]"#),
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
        Ok(ValuesFn { value }.as_expr())
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

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        // The type of all possible values is merged together to get a more specific value than just `TypeDef::any()`
        let merged_kind = self
            .value
            .type_def(state)
            .kind()
            .as_object()
            .unwrap()
            .reduced_kind();
        TypeDef::array(Collection::empty().with_unknown(merged_kind)).infallible()
    }
}
