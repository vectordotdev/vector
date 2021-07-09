use vrl::prelude::*;

use datadog_search_syntax::{normalize_fields, parse, Field, QueryNode};
use lookup::{parser::parse_lookup, LookupBuf};

#[derive(Clone, Copy, Debug)]
pub struct MatchDatadogQuery;

impl Function for MatchDatadogQuery {
    fn identifier(&self) -> &'static str {
        "match_datadog_query"
    }

    fn examples(&self) -> &'static [Example] {
        todo!()
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let query_value = arguments.required_literal("query")?.to_value();

        let query = query_value
            .try_bytes_utf8_lossy()
            .expect("datadog search query not bytes");

        // Compile the Datadog search query to AST.
        let node = parse(&query).map_err(|e| {
            Box::new(ExpressionError::from(e.to_string())) as Box<dyn DiagnosticError>
        })?;

        Ok(Box::new(MatchDatadogQueryFn { value, node }))
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
struct MatchDatadogQueryFn {
    value: Box<dyn Expression>,
    node: QueryNode,
}

impl Expression for MatchDatadogQueryFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_object()?;

        Ok(matches_vrl_object(&self.node, Value::Object(value)).into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        type_def()
    }
}

fn type_def() -> TypeDef {
    TypeDef::new().infallible().boolean()
}

fn matches_vrl_object(node: &QueryNode, obj: Value) -> bool {
    match node {
        QueryNode::MatchNoDocs => false,
        QueryNode::MatchAllDocs => true,
        QueryNode::AttributeExists { attr } => exists(attr, obj),
        QueryNode::AttributeMissing { attr } => !exists(attr, obj),
        _ => false,
    }
}

fn exists<T: AsRef<str>>(attr: T, obj: Value) -> bool {
    normalize_fields(attr).into_iter().any(|f| match f {
        Field::Default(p) | Field::Reserved(p) | Field::Facet(p) => {
            let buf = match parse_lookup(p.as_str()) {
                Ok(l) => l.into_buf(),
                Err(_) => return false,
            };

            obj.get_by_path(&buf).is_some()
        }
        Field::Tag(t) => {
            let buf = LookupBuf::from("tags");

            match obj.get_by_path(&buf) {
                Some(Value::Array(v)) => v.contains(&Value::Bytes(t.into())),
                _ => false,
            }
        }
    })
}

fn equals<T: AsRef<str>>(attr: T, obj: Value) -> bool {
    normalize_fields(attr).into_iter().any(|f| match f {
        Field::Default(p) | Field::Reserved(p) | Field::Facet(p) => {
            let buf = match parse_lookup(p.as_str()) {
                Ok(l) => l.into_buf(),
                Err(_) => return false,
            };

            obj.get_by_path(&buf).is_some()
        }
        Field::Tag(t) => {
            let buf = LookupBuf::from("tags");

            match obj.get_by_path(&buf) {
                Some(Value::Array(v)) => v.contains(&Value::Bytes(t.into())),
                _ => false,
            }
        }
    })
}

#[cfg(test)]
mod test {
    use super::*;

    test_function![
        match_datadog_query => MatchDatadogQuery;

        message_exists {
            args: func_args![value: value!({"message": "test message"}), query: "_exists_:message"],
            want: Ok(true),
            tdef: type_def(),
        }

        facet_exists {
            args: func_args![value: value!({"custom": {"a": "value" }}), query: "_exists_:@a"],
            want: Ok(true),
            tdef: type_def(),
        }

        tag_exists {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "_exists_:a"],
            want: Ok(true),
            tdef: type_def(),
        }

        message_missing {
            args: func_args![value: value!({}), query: "_missing_:message"],
            want: Ok(true),
            tdef: type_def(),
        }

        facet_missing {
            args: func_args![value: value!({"custom": {"b": "value" }}), query: "_missing_:@a"],
            want: Ok(true),
            tdef: type_def(),
        }

        tag_missing {
            args: func_args![value: value!({"tags": ["b","c"]}), query: "_missing_:a"],
            want: Ok(true),
            tdef: type_def(),
        }
    ];
}
