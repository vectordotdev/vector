use super::{node::QueryNode, parse};
use vrl::{
    function::{ArgumentList, Compiled, Example},
    prelude::*,
    Context,
};
use vrl_compiler::{expression::Expression, value::kind, Function, Parameter};

#[derive(Clone, Copy, Debug)]
pub struct DatadogSearch;

impl Function for DatadogSearch {
    fn identifier(&self) -> &'static str {
        "datadog_search"
    }

    fn examples(&self) -> &'static [Example] {
        todo!()
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let query = arguments
            .required_literal("query")?
            .to_value()
            .try_bytes_utf8_lossy()
            .expect("datadog search query not bytes");

        // Compile the Datadog search query to AST.
        let node = parse(&query).map_err(|e| e.to_string().into())?;

        Ok(Box::new(DatadogSearchFn { value, node }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::OBJECT,
                required: true,
            },
            Parameter {
                keyword: "query",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct DatadogSearchFn {
    value: Box<dyn Expression>,
    node: QueryNode,
}

impl Expression for DatadogSearchFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_object()?;

        Ok(self.node.matches_vrl_object(value).into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}
