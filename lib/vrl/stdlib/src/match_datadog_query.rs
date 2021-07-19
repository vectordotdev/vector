use vrl::prelude::*;

use cached::{proc_macro::cached, SizedCache};
use datadog_search_syntax::{
    normalize_fields, parse, Comparison, ComparisonValue, Field, QueryNode,
};
use lookup::{parser::parse_lookup, LookupBuf};
use regex::Regex;
use std::borrow::{Borrow, Cow};

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

        // Query should always be a string.
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

/// Match the parsed node against the provided VRL `Value`, per the query type.
fn matches_vrl_object(node: &QueryNode, obj: Value) -> bool {
    match node {
        QueryNode::MatchNoDocs => false,
        QueryNode::MatchAllDocs => true,
        QueryNode::AttributeExists { attr } => exists(attr, obj),
        QueryNode::AttributeMissing { attr } => !exists(attr, obj),
        QueryNode::AttributeTerm { attr, value }
        | QueryNode::QuotedAttribute {
            attr,
            phrase: value,
        } => equals(attr, obj, value),
        QueryNode::AttributeComparison {
            attr,
            comparator,
            value,
        } => compare(attr, obj, comparator, value),
        QueryNode::AttributePrefix { attr, prefix } => wildcard_match(attr, obj, prefix),
        _ => false,
    }
}

/// Returns true if the field exists. Also takes a `Value` to match against tag types.
fn exists<T: AsRef<str>>(attr: T, obj: Value) -> bool {
    normalize_fields(attr).into_iter().any(|field| {
        let path = match lookup_field(&field) {
            Some(path) => path,
            None => return false,
        };

        match field {
            // Tags exist by element value.
            Field::Tag(t) => match obj.get_by_path(&path) {
                Some(Value::Array(v)) => v.contains(&Value::Bytes(t.into())),
                _ => false,
            },
            // Other fields exist by path.
            _ => obj.get_by_path(&path).is_some(),
        }
    })
}

#[cached(
    type = "SizedCache<String, Regex>",
    create = "{ SizedCache::with_size(10) }",
    convert = r#"{ to_match.to_owned() }"#
)]
/// Returns compiled wildcard regex. Cached to avoid recompilation in hot paths.
fn wildcard_regex(to_match: &str) -> Regex {
    Regex::new(&format!(
        r#"\b{}\b"#,
        regex::escape(to_match).replace("\\*", ".*")
    ))
    .expect("invalid wildcard regex")
}

/// Returns true if the provided VRL `Value` matches the `to_match` string.
fn equals<T: AsRef<str>>(attr: T, obj: Value, to_match: &str) -> bool {
    each_field(attr, obj, |field, value| {
        match field {
            // Tags are compared by element key:value.
            Field::Tag(tag) => match value {
                Value::Array(v) => {
                    v.contains(&Value::Bytes(format!("{}:{}", tag, to_match).into()))
                }
                _ => false,
            },
            // Default fields are compared by word boundary.
            Field::Default(_) => match value {
                Value::Bytes(val) => {
                    let re = wildcard_regex(to_match);
                    re.is_match(&String::from_utf8_lossy(val))
                }
                _ => false,
            },
            // Everything else is matched by string equality.
            _ => string_value(value) == to_match,
        }
    })
}

/// Compares the field path as numeric or string depending on the field type.
fn compare<T: AsRef<str>>(
    attr: T,
    obj: Value,
    comparator: &Comparison,
    comparison_value: &ComparisonValue,
) -> bool {
    each_field(attr, obj, |field, value| {
        // If the field is a default tag, then it must be interpreted as a string
        // for the purpose of making comparisons. Coerce the `ComparisonValue` to a string.
        let comparison_value = if matches!(field, Field::Tag(_))
            && !matches!(comparison_value, ComparisonValue::String(_))
        {
            Cow::Owned(ComparisonValue::String(comparison_value.to_string()))
        } else {
            Cow::Borrowed(comparison_value)
        };

        match comparison_value.borrow() {
            ComparisonValue::Float(v) => {
                let value = match value {
                    Value::Float(value) => value.into_inner(),
                    _ => return false,
                };

                match comparator {
                    Comparison::Lt => value < *v,
                    Comparison::Lte => value <= *v,
                    Comparison::Gt => value > *v,
                    Comparison::Gte => value >= *v,
                }
            }

            ComparisonValue::Integer(v) => {
                let value = match value {
                    Value::Integer(value) => value,
                    _ => return false,
                };

                match comparator {
                    Comparison::Lt => value < v,
                    Comparison::Lte => value <= v,
                    Comparison::Gt => value > v,
                    Comparison::Gte => value >= v,
                }
            }

            ComparisonValue::String(v) => {
                let value = string_value(value);

                match comparator {
                    Comparison::Lt => value < *v,
                    Comparison::Lte => value <= *v,
                    Comparison::Gt => value > *v,
                    Comparison::Gte => value >= *v,
                }
            }

            ComparisonValue::Unbounded => false,
        }
    })
}

/// Returns true if the provided `Value` matches the prefix.
fn wildcard_match<T: AsRef<str>>(attr: T, obj: Value, prefix: &str) -> bool {
    each_field(attr, obj, |field, value| match field {
        Field::Default(_) => {
            let re = wildcard_regex(&format!("{}*", prefix));
            re.is_match(&string_value(value))
        }
        Field::Tag(tag) => match value {
            Value::Array(v) => v
                .iter()
                .any(|v| string_value(v).starts_with(&format!("{}:{}", tag, prefix))),
            _ => false,
        },
        _ => string_value(value).starts_with(prefix),
    })
}

/// Iterator over normalized fields, passing the field look-up and its Value to the
/// provided `value_fn`.
fn each_field<T: AsRef<str>>(
    attr: T,
    obj: Value,
    value_fn: impl Fn(Field, &Value) -> bool,
) -> bool {
    normalize_fields(attr).into_iter().any(|field| {
        // Look up the field. For tags, this will literally be "tags". For all other
        // types, this will be based on the the string field nane.
        let path = match lookup_field(&field) {
            Some(b) => b,
            _ => return false,
        };

        // Get the value by path, or return early with `false` if it doesn't exist.
        let value = match obj.get_by_path(&path) {
            Some(v) => v,
            _ => return false,
        };

        // Provide the field and value to the callback.
        value_fn(field, value)
    })
}

/// If the provided field is a `Field::Tag`, will return a "tags" lookup buf. Otherwise,
/// parses the field and returns a lookup buf is the lookup itself is valid.
fn lookup_field(field: &Field) -> Option<LookupBuf> {
    match field {
        Field::Default(p) | Field::Reserved(p) | Field::Facet(p) => {
            Some(parse_lookup(p.as_str()).ok()?.into_buf())
        }
        Field::Tag(_) => Some(LookupBuf::from("tags")),
    }
}

/// Returns a string value from a VRL `Value`. This differs from the regular `Display`
/// implementation by treating Bytes values as special-- returning the UTF8 representation
/// instead of the raw control characters.
fn string_value(value: &Value) -> String {
    match value {
        Value::Bytes(val) => String::from_utf8_lossy(val).to_string(),
        _ => value.to_string(),
    }
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

        equals_message {
            args: func_args![value: value!({"message": "match by word boundary"}), query: "match"],
            want: Ok(true),
            tdef: type_def(),
        }

        equals_message_no_match {
            args: func_args![value: value!({"message": "another value"}), query: "match"],
            want: Ok(false),
            tdef: type_def(),
        }

        equals_tag {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "y:2"],
            want: Ok(true),
            tdef: type_def(),
        }

        equals_tag_no_match {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "y:3"],
            want: Ok(false),
            tdef: type_def(),
        }

        equals_facet {
            args: func_args![value: value!({"custom": {"z": 1}}), query: "@z:1"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_message {
            args: func_args![value: value!({"message": "vector"}), query: "vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_suffix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_suffix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "@a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "@a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }
    ];
}
