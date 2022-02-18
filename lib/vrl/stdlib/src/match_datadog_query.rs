use std::borrow::Cow;

use datadog_filter::{
    fast_matcher::{self, Mode, Op},
    regex::wildcard_regex,
    Resolver,
};
use datadog_search_syntax::{parse, Comparison, ComparisonValue, Field};
use lookup_lib::{parser::parse_lookup, LookupBuf};
use regex::bytes;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct MatchDatadogQuery;

impl Function for MatchDatadogQuery {
    fn identifier(&self) -> &'static str {
        "match_datadog_query"
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "OR query",
                source: r#"match_datadog_query({"message": "contains this and that"}, "this OR that")"#,
                result: Ok("true"),
            },
            Example {
                title: "AND query",
                source: r#"match_datadog_query({"message": "contains only this"}, "this AND that")"#,
                result: Ok("false"),
            },
            Example {
                title: "Facet wildcard",
                source: r#"match_datadog_query({"custom": {"name": "vector"}}, "@name:vec*")"#,
                result: Ok("true"),
            },
            Example {
                title: "Tag range",
                source: r#"match_datadog_query({"tags": ["a:x", "b:y", "c:z"]}, s'b:["x" TO "z"]')"#,
                result: Ok("true"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let query_value = arguments.required_literal("query")?.to_value();

        // Query should always be a string.
        let query = query_value
            .try_bytes_utf8_lossy()
            .expect("datadog search query should be a UTF8 string");

        // Compile the Datadog search query to AST.
        let node = parse(&query).map_err(|e| {
            Box::new(ExpressionError::from(e.to_string())) as Box<dyn DiagnosticError>
        })?;

        // Build the matcher function that accepts a VRL event value. This will parse the `node`
        // at boot-time and return a boxed func that contains just the logic required to match a
        // VRL `Value` against the Datadog Search Syntax literal.
        let filter = fast_matcher::build_matcher(&node, &VrlFilter::default());

        Ok(Box::new(MatchDatadogQueryFn { value, filter }))
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _info: &FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("query", Some(expr)) => {
                let query_value =
                    expr.as_value()
                        .ok_or_else(|| vrl::function::Error::UnexpectedExpression {
                            keyword: "query",
                            expected: "literal",
                            expr: expr.clone(),
                        })?;

                let query = query_value
                    .try_bytes_utf8_lossy()
                    .expect("datadog search query should be a UTF8 string");

                // Compile the Datadog search query to AST.
                let node = parse(&query).map_err(|e| {
                    Box::new(ExpressionError::from(e.to_string())) as Box<dyn DiagnosticError>
                })?;

                // Build the matcher function that accepts a VRL event value. This will parse the `node`
                // at boot-time and return a boxed func that contains just the logic required to match a
                // VRL `Value` against the Datadog Search Syntax literal.
                let filter = fast_matcher::build_matcher(&node, &VrlFilter::default());

                Ok(Some(
                    Box::new(filter) as Box<dyn std::any::Any + Send + Sync>
                ))
            }
            _ => Ok(None),
        }
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
    filter: fast_matcher::FastMatcher,
}

impl Expression for MatchDatadogQueryFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        // Provide the current VRL event `Value` to the matcher function to determine
        // whether the data matches the given Datadog Search syntax literal.
        Ok(VrlFilter::run(&self.filter, &value).into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        type_def()
    }
}

fn type_def() -> TypeDef {
    TypeDef::boolean().infallible()
}

#[derive(Default, Clone)]
struct VrlFilter;

/// Implements `Resolver`, which translates Datadog Search Syntax literal names into
/// fields.
impl Resolver for VrlFilter {}

impl VrlFilter {
    pub fn run(matcher: &fast_matcher::FastMatcher, obj: &Value) -> bool {
        match &matcher.mode {
            Mode::One(op) => exec(op, obj),
            Mode::Any(ops) => ops.iter().any(|op| exec(op, obj)),
            Mode::All(ops) => ops.iter().all(|op| exec(op, obj)),
        }
    }
}

fn exec(op: &fast_matcher::Op, obj: &Value) -> bool {
    match op {
        Op::True => true,
        Op::False => true,
        Op::Exists(field) => exists(field, obj),
        Op::NotExists(field) => !exists(field, obj),
        Op::Equals { field, value } => equals(field, value, obj),
        Op::TagExists(to_match) => tag_exists(to_match, obj),
        Op::RegexMatch { field, re } => regex_match(field, re, obj),
        Op::Prefix(field, value) => prefix(field, value, obj),
        Op::Wildcard(field, value) => wildcard(field, value, obj),
        Op::Compare(field, comparison, comparison_value) => {
            compare(field, *comparison, comparison_value, obj)
        }
        Op::Range {
            field,
            lower,
            lower_inclusive,
            upper,
            upper_inclusive,
        } => {
            match (&lower, &upper) {
                // If both bounds are wildcards, just check that the field exists to catch the
                // special case for "tags".
                (ComparisonValue::Unbounded, ComparisonValue::Unbounded) => exists(field, obj),
                // Unbounded lower.
                (ComparisonValue::Unbounded, _) => {
                    let op = if *upper_inclusive {
                        Comparison::Lte
                    } else {
                        Comparison::Lt
                    };
                    compare(field, op, upper, obj)
                }
                // Unbounded upper.
                (_, ComparisonValue::Unbounded) => {
                    let op = if *lower_inclusive {
                        Comparison::Gte
                    } else {
                        Comparison::Gt
                    };

                    compare(field, op, lower, obj)
                }
                // Definitive range.
                _ => {
                    let lower_op = if *lower_inclusive {
                        Comparison::Gte
                    } else {
                        Comparison::Gt
                    };

                    let upper_op = if *upper_inclusive {
                        Comparison::Lte
                    } else {
                        Comparison::Lt
                    };

                    compare(field, lower_op, lower, obj) && compare(field, upper_op, upper, obj)
                }
            }
        }
        Op::Not(matcher) => !VrlFilter::run(matcher, obj),
        Op::Nested(matcher) => VrlFilter::run(matcher, obj),
    }
}

fn resolve_value<F>(buf: &LookupBuf, obj: &Value, func: F) -> bool
where
    F: Fn(&Value) -> bool,
{
    match obj.get_by_path(buf) {
        Some(value) => func(value),
        _ => false,
    }
}

fn tag_exists(to_match: &str, obj: &Value) -> bool {
    let buf = parse_lookup("tags")
        .expect("should parse lookup buf")
        .into_buf();

    resolve_value(&buf, obj, |val: &Value| match val {
        Value::Array(values) => values.iter().any(|value| {
            if let Value::Bytes(bytes) = value {
                bytes == to_match.as_bytes()
            } else {
                false
            }
        }),
        _ => false,
    })
}

fn exists(field: &Field, obj: &Value) -> bool {
    let buf = lookup_field(field);

    match field {
        // Tags need to check the element value.
        Field::Tag(tag) => {
            resolve_value(&buf, obj, |val: &Value| {
                match val {
                    Value::Array(v) => {
                        let starts_with = format!("{}:", tag);
                        v.iter().any(|v| {
                            let str_value = string_value(v);

                            // The tag matches using either 'key' or 'key:value' syntax.
                            str_value == tag.as_str() || str_value.starts_with(&starts_with)
                        })
                    }
                    _ => false,
                }
            })
        }
        // Literal field 'tags' needs to be compared by key.
        Field::Reserved(f) if f == "tags" => resolve_value(&buf, obj, |val: &Value| match val {
            Value::Array(v) => v.iter().any(|v| v == val),
            _ => false,
        }),
        // Other field types have already resolved at this point, so just return true.
        _ => resolve_value(&buf, obj, |_val: &Value| true),
    }
}

/// Returns compiled word boundary regex.
#[must_use]
pub fn word_regex(to_match: &str) -> bytes::Regex {
    bytes::Regex::new(&format!(
        r#"\b{}\b"#,
        regex::escape(to_match).replace("\\*", ".*")
    ))
    .expect("invalid wildcard regex")
}

fn equals(field: &Field, to_match: &str, obj: &Value) -> bool {
    let buf = lookup_field(field);

    match field {
        // Default fields are compared by word boundary.
        Field::Default(_) => resolve_value(&buf, obj, |val: &Value| match val {
            Value::Bytes(b) => {
                let re = word_regex(to_match);
                re.is_match(b)
            }
            _ => false,
        }),
        // A literal "tags" field should match by key.
        Field::Reserved(f) if f == "tags" => resolve_value(&buf, obj, |val: &Value| match val {
            Value::Array(v) => {
                v.contains(&Value::Bytes(Bytes::copy_from_slice(to_match.as_bytes())))
            }
            _ => false,
        }),
        // Individual tags are compared by element key:value.
        Field::Tag(tag) => {
            let value_bytes = Value::Bytes(format!("{}:{}", tag, to_match).into());

            resolve_value(&buf, obj, |val: &Value| match val {
                Value::Array(v) => v.contains(&value_bytes),
                _ => false,
            })
        }
        // Everything else is matched by string equality.
        _ => resolve_value(&buf, obj, |val: &Value| string_value(val) == to_match),
    }
}

fn prefix(field: &Field, pfx: &str, obj: &Value) -> bool {
    let buf = lookup_field(field);

    match field {
        // Default fields are matched by word boundary.
        Field::Default(_) => resolve_value(&buf, obj, |val: &Value| {
            let re = word_regex(&format!("{}*", pfx));
            re.is_match(string_value(val).as_bytes())
        }),
        // Tags are recursed until a match is found.
        Field::Tag(tag) => resolve_value(&buf, obj, |val: &Value| match val {
            Value::Array(v) => {
                let starts_with = format!("{}:{}", tag, pfx);
                v.iter().any(|v| string_value(v).starts_with(&starts_with))
            }
            _ => false,
        }),
        // All other field types are compared by complete value.
        _ => resolve_value(&buf, obj, |val: &Value| string_value(val).starts_with(&pfx)),
    }
}

fn regex_match(field: &Field, re: &regex::Regex, obj: &Value) -> bool {
    let buf = lookup_field(field);

    resolve_value(&buf, obj, |val: &Value| match val {
        Value::Bytes(s) => {
            if let Ok(s) = std::str::from_utf8(s) {
                re.is_match(s)
            } else {
                false
            }
        }
        _ => false,
    })
}

fn wildcard(field: &Field, wildcard: &str, obj: &Value) -> bool {
    let buf = lookup_field(field);

    match field {
        Field::Default(_) => resolve_value(&buf, obj, |val: &Value| {
            let re = word_regex(wildcard);
            re.is_match(string_value(val).as_bytes())
        }),
        Field::Tag(tag) => resolve_value(&buf, obj, |val: &Value| match val {
            Value::Array(v) => {
                let re = wildcard_regex(&format!("{}:{}", tag, wildcard));
                v.iter().any(|v| re.is_match(&string_value(v)))
            }
            _ => false,
        }),
        _ => resolve_value(&buf, obj, |val: &Value| {
            let re = wildcard_regex(wildcard);
            re.is_match(&string_value(val))
        }),
    }
}

fn compare(
    field: &Field,
    comparator: Comparison,
    comparison_value: &ComparisonValue,
    obj: &Value,
) -> bool {
    let buf = lookup_field(field);
    let rhs = Cow::from(comparison_value.to_string());

    match field {
        // Facets are compared numerically if the value is numeric, or as strings otherwise.
        Field::Facet(_) => {
            resolve_value(&buf, obj, |val: &Value| match (val, &comparison_value) {
                // Integers.
                (Value::Integer(lhs), ComparisonValue::Integer(rhs)) => match comparator {
                    Comparison::Lt => *lhs < *rhs,
                    Comparison::Lte => *lhs <= *rhs,
                    Comparison::Gt => *lhs > *rhs,
                    Comparison::Gte => *lhs >= *rhs,
                },
                // Integer value - Float boundary
                (Value::Integer(lhs), ComparisonValue::Float(rhs)) => match comparator {
                    Comparison::Lt => (*lhs as f64) < *rhs,
                    Comparison::Lte => *lhs as f64 <= *rhs,
                    Comparison::Gt => *lhs as f64 > *rhs,
                    Comparison::Gte => *lhs as f64 >= *rhs,
                },
                // Floats.
                (Value::Float(lhs), ComparisonValue::Float(rhs)) => {
                    let lhs = lhs.into_inner();
                    match comparator {
                        Comparison::Lt => lhs < *rhs,
                        Comparison::Lte => lhs <= *rhs,
                        Comparison::Gt => lhs > *rhs,
                        Comparison::Gte => lhs >= *rhs,
                    }
                }
                // Float value - Integer boundary
                (Value::Float(lhs), ComparisonValue::Integer(rhs)) => {
                    let lhs = lhs.into_inner();
                    match comparator {
                        Comparison::Lt => lhs < *rhs as f64,
                        Comparison::Lte => lhs <= *rhs as f64,
                        Comparison::Gt => lhs > *rhs as f64,
                        Comparison::Gte => lhs >= *rhs as f64,
                    }
                }
                // Where the rhs is a string ref, the lhs is coerced into a string.
                (_, ComparisonValue::String(rhs)) => {
                    let lhs = string_value(val);
                    let rhs = Cow::from(rhs);

                    match comparator {
                        Comparison::Lt => lhs < rhs,
                        Comparison::Lte => lhs <= rhs,
                        Comparison::Gt => lhs > rhs,
                        Comparison::Gte => lhs >= rhs,
                    }
                }
                // Otherwise, compare directly as strings.
                _ => {
                    let lhs = string_value(val);

                    match comparator {
                        Comparison::Lt => lhs < rhs,
                        Comparison::Lte => lhs <= rhs,
                        Comparison::Gt => lhs > rhs,
                        Comparison::Gte => lhs >= rhs,
                    }
                }
            })
        }
        // Tag values need extracting by "key:value" to be compared.
        Field::Tag(_) => resolve_value(&buf, obj, |val: &Value| match val {
            Value::Array(v) => v.iter().any(|v| match string_value(v).split_once(":") {
                Some((_, lhs)) => {
                    let lhs = Cow::from(lhs);

                    match comparator {
                        Comparison::Lt => lhs < rhs,
                        Comparison::Lte => lhs <= rhs,
                        Comparison::Gt => lhs > rhs,
                        Comparison::Gte => lhs >= rhs,
                    }
                }
                _ => false,
            }),
            _ => false,
        }),
        // All other tag types are compared by string.
        _ => resolve_value(&buf, obj, |val: &Value| {
            let lhs = string_value(val);

            match comparator {
                Comparison::Lt => lhs < rhs,
                Comparison::Lte => lhs <= rhs,
                Comparison::Gt => lhs > rhs,
                Comparison::Gte => lhs >= rhs,
            }
        }),
    }
}

/// If the provided field is a `Field::Tag`, will return a "tags" lookup buf. Otherwise,
/// parses the field and returns a lookup buf is the lookup itself is valid.
fn lookup_field(field: &Field) -> LookupBuf {
    match field {
        Field::Default(p) | Field::Reserved(p) | Field::Facet(p) => parse_lookup(p.as_str())
            .expect("should parse lookup buf")
            .into_buf(),
        Field::Tag(_) => LookupBuf::from("tags"),
    }
}

/// Returns a string value from a VRL `Value`. This differs from the regular `Display`
/// implementation by treating Bytes values as special-- returning the UTF8 representation
/// instead of the raw control characters.
fn string_value(value: &Value) -> Cow<str> {
    match value {
        Value::Bytes(val) => String::from_utf8_lossy(val),
        _ => Cow::from(value.to_string()),
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

        not_message_exists {
            args: func_args![value: value!({"message": "test message"}), query: "NOT _exists_:message"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_message_exists {
            args: func_args![value: value!({"message": "test message"}), query: "-_exists_:message"],
            want: Ok(false),
            tdef: type_def(),
        }

        facet_exists {
            args: func_args![value: value!({"custom": {"a": "value" }}), query: "_exists_:@a"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_facet_exists {
            args: func_args![value: value!({"custom": {"a": "value" }}), query: "NOT _exists_:@a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_facet_exists {
            args: func_args![value: value!({"custom": {"a": "value" }}), query: "-_exists_:@a"],
            want: Ok(false),
            tdef: type_def(),
        }

        tag_bare {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "tags:a"],
            want: Ok(true),
            tdef: type_def(),
        }

        tag_bare_no_match {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "tags:d"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_tag_bare {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "NOT tags:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_tag_bare {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "-tags:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        tag_exists_bare {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "_exists_:a"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_tag_exists_bare {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "NOT _exists_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_tag_exists_bare {
            args: func_args![value: value!({"tags": ["a","b","c"]}), query: "-_exists_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        tag_exists {
            args: func_args![value: value!({"tags": ["a:1","b:2","c:3"]}), query: "_exists_:a"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_tag_exists {
            args: func_args![value: value!({"tags": ["a:1","b:2","c:3"]}), query: "NOT _exists_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_tag_exists {
            args: func_args![value: value!({"tags": ["a:1","b:2","c:3"]}), query: "-_exists_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        message_missing {
            args: func_args![value: value!({}), query: "_missing_:message"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_message_missing {
            args: func_args![value: value!({}), query: "NOT _missing_:message"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_message_missing {
            args: func_args![value: value!({}), query: "-_missing_:message"],
            want: Ok(false),
            tdef: type_def(),
        }

        facet_missing {
            args: func_args![value: value!({"custom": {"b": "value" }}), query: "_missing_:@a"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_facet_missing {
            args: func_args![value: value!({"custom": {"b": "value" }}), query: "NOT _missing_:@a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_facet_missing {
            args: func_args![value: value!({"custom": {"b": "value" }}), query: "-_missing_:@a"],
            want: Ok(false),
            tdef: type_def(),
        }

        tag_bare_missing {
            args: func_args![value: value!({"tags": ["b","c"]}), query: "_missing_:a"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_tag_bare_missing {
            args: func_args![value: value!({"tags": ["b","c"]}), query: "NOT _missing_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_tag_bare_missing {
            args: func_args![value: value!({"tags": ["b","c"]}), query: "-_missing_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        tag_missing {
            args: func_args![value: value!({"tags": ["b:1","c:2"]}), query: "_missing_:a"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_tag_missing {
            args: func_args![value: value!({"tags": ["b:1","c:2"]}), query: "NOT _missing_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_tag_missing {
            args: func_args![value: value!({"tags": ["b:1","c:2"]}), query: "-_missing_:a"],
            want: Ok(false),
            tdef: type_def(),
        }

        equals_message {
            args: func_args![value: value!({"message": "match by word boundary"}), query: "match"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_equals_message {
            args: func_args![value: value!({"message": "match by word boundary"}), query: "NOT match"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_equals_message {
            args: func_args![value: value!({"message": "match by word boundary"}), query: "-match"],
            want: Ok(false),
            tdef: type_def(),
        }

        equals_message_no_match {
            args: func_args![value: value!({"message": "another value"}), query: "match"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_equals_message_no_match {
            args: func_args![value: value!({"message": "another value"}), query: "NOT match"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_equals_message_no_match {
            args: func_args![value: value!({"message": "another value"}), query: "-match"],
            want: Ok(true),
            tdef: type_def(),
        }

        equals_tag {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "y:2"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_equals_tag {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "NOT y:2"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_equals_tag {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "-y:2"],
            want: Ok(false),
            tdef: type_def(),
        }

        equals_tag_no_match {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "y:3"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_equals_tag_no_match {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "NOT y:3"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_equals_tag_no_match {
            args: func_args![value: value!({"tags": ["x:1", "y:2", "z:3"]}), query: "-y:3"],
            want: Ok(true),
            tdef: type_def(),
        }

        equals_facet {
            args: func_args![value: value!({"custom": {"z": 1}}), query: "@z:1"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_equals_facet {
            args: func_args![value: value!({"custom": {"z": 1}}), query: "NOT @z:1"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_equals_facet {
            args: func_args![value: value!({"custom": {"z": 1}}), query: "-@z:1"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_prefix_message {
            args: func_args![value: value!({"message": "vector"}), query: "*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_prefix_message {
            args: func_args![value: value!({"message": "vector"}), query: "NOT *tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_prefix_message {
            args: func_args![value: value!({"message": "vector"}), query: "-*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_prefix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_prefix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "NOT *tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_prefix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "-*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_prefix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "a:*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_prefix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "NOT a:*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_prefix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "-a:*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_prefix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "a:*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_prefix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "NOT a:*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_prefix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "-a:*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_prefix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "@a:*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_prefix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "NOT @a:*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_prefix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "-@a:*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_prefix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "@a:*tor"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_prefix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "NOT @a:*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_prefix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "-@a:*tor"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_message {
            args: func_args![value: value!({"message": "vector"}), query: "vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_suffix_message {
            args: func_args![value: value!({"message": "vector"}), query: "NOT vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_suffix_message {
            args: func_args![value: value!({"message": "vector"}), query: "-vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_suffix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_suffix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "NOT vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_suffix_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "-vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_suffix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "NOT a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_suffix_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "-a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_suffix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_suffix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "NOT a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_suffix_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "-a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_suffix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "@a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_suffix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "NOT @a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_suffix_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "-@a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_suffix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "@a:vec*"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_suffix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "NOT @a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_suffix_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "-@a:vec*"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_multiple_message {
            args: func_args![value: value!({"message": "vector"}), query: "v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_multiple_message {
            args: func_args![value: value!({"message": "vector"}), query: "NOT v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_multiple_message {
            args: func_args![value: value!({"message": "vector"}), query: "-v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_multiple_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_multiple_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "NOT v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_multiple_message_no_match {
            args: func_args![value: value!({"message": "torvec"}), query: "-v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_multiple_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "a:v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_multiple_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "NOT a:v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_multiple_tag {
            args: func_args![value: value!({"tags": ["a:vector"]}), query: "-a:v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_multiple_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "a:v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_multiple_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "NOT a:v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_multiple_tag_no_match {
            args: func_args![value: value!({"tags": ["b:vector"]}), query: "-a:v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        wildcard_multiple_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "@a:v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_wildcard_multiple_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "NOT @a:v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_wildcard_multiple_facet {
            args: func_args![value: value!({"custom": {"a": "vector"}}), query: "-@a:v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        wildcard_multiple_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "@a:v*c*r"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_wildcard_multiple_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "NOT @a:v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_wildcard_multiple_facet_no_match {
            args: func_args![value: value!({"custom": {"b": "vector"}}), query: "-@a:v*c*r"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_message_unbounded {
            args: func_args![value: value!({"message": "1"}), query: "[* TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_unbounded {
            args: func_args![value: value!({"message": "1"}), query: "NOT [* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_unbounded {
            args: func_args![value: value!({"message": "1"}), query: "-[* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_lower_bound {
            args: func_args![value: value!({"message": "400"}), query: "[4 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_lower_bound {
            args: func_args![value: value!({"message": "400"}), query: "NOT [4 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_lower_bound {
            args: func_args![value: value!({"message": "400"}), query: "-[4 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_lower_bound_no_match {
            args: func_args![value: value!({"message": "400"}), query: "[50 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_message_lower_bound_no_match {
            args: func_args![value: value!({"message": "400"}), query: "NOT [50 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_message_lower_bound_no_match {
            args: func_args![value: value!({"message": "400"}), query: "-[50 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_message_lower_bound_string {
            args: func_args![value: value!({"message": "400"}), query: r#"["4" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_lower_bound_string {
            args: func_args![value: value!({"message": "400"}), query: r#"NOT ["4" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_lower_bound_string {
            args: func_args![value: value!({"message": "400"}), query: r#"-["4" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_lower_bound_string_no_match {
            args: func_args![value: value!({"message": "400"}), query: r#"["50" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_message_lower_bound_string_no_match {
            args: func_args![value: value!({"message": "400"}), query: r#"NOT ["50" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_message_lower_bound_string_no_match {
            args: func_args![value: value!({"message": "400"}), query: r#"-["50" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_message_upper_bound {
            args: func_args![value: value!({"message": "300"}), query: "[* TO 4]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_upper_bound {
            args: func_args![value: value!({"message": "300"}), query: "NOT [* TO 4]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_upper_bound {
            args: func_args![value: value!({"message": "300"}), query: "-[* TO 4]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_upper_bound_no_match {
            args: func_args![value: value!({"message": "50"}), query: "[* TO 400]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_message_upper_bound_no_match {
            args: func_args![value: value!({"message": "50"}), query: "NOT [* TO 400]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_message_upper_bound_no_match {
            args: func_args![value: value!({"message": "50"}), query: "-[* TO 400]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_message_upper_bound_string {
            args: func_args![value: value!({"message": "300"}), query: r#"[* TO "4"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_upper_bound_string {
            args: func_args![value: value!({"message": "300"}), query: r#"NOT [* TO "4"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_upper_bound_string {
            args: func_args![value: value!({"message": "300"}), query: r#"-[* TO "4"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_upper_bound_string_no_match {
            args: func_args![value: value!({"message": "50"}), query: r#"[* TO "400"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_message_upper_bound_string_no_match {
            args: func_args![value: value!({"message": "50"}), query: r#"NOT [* TO "400"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_message_upper_bound_string_no_match {
            args: func_args![value: value!({"message": "50"}), query: r#"NOT [* TO "400"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_message_between {
            args: func_args![value: value!({"message": 500}), query: "[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_between {
            args: func_args![value: value!({"message": 500}), query: "NOT [1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_between {
            args: func_args![value: value!({"message": 500}), query: "-[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_between_no_match {
            args: func_args![value: value!({"message": 70}), query: "[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_message_between_no_match {
            args: func_args![value: value!({"message": 70}), query: "NOT [1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_message_between_no_match {
            args: func_args![value: value!({"message": 70}), query: "-[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_message_between_string {
            args: func_args![value: value!({"message": "500"}), query: r#"["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_message_between_string {
            args: func_args![value: value!({"message": "500"}), query: r#"NOT ["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_message_between_string {
            args: func_args![value: value!({"message": "500"}), query: r#"-["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_message_between_no_match_string {
            args: func_args![value: value!({"message": "70"}), query: r#"["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_message_between_no_match_string {
            args: func_args![value: value!({"message": "70"}), query: r#"NOT ["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_message_between_no_match_string {
            args: func_args![value: value!({"message": "70"}), query: r#"-["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_key {
            args: func_args![value: value!({"tags": ["a"]}), query: "a:[* TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_key_no_match {
            args: func_args![value: value!({"tags": ["b"]}), query: "a:[* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_unbounded {
            args: func_args![value: value!({"tags": ["a:1"]}), query: "a:[* TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_unbounded {
            args: func_args![value: value!({"tags": ["a:1"]}), query: "NOT a:[* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_unbounded {
            args: func_args![value: value!({"tags": ["a:1"]}), query: "-a:[* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_lower_bound {
            args: func_args![value: value!({"tags": ["a:400"]}), query: "a:[4 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_lower_bound {
            args: func_args![value: value!({"tags": ["a:400"]}), query: "NOT a:[4 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_lower_bound {
            args: func_args![value: value!({"tags": ["a:400"]}), query: "-a:[4 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_lower_bound_no_match {
            args: func_args![value: value!({"tags": ["a:400"]}), query: "a:[50 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_tag_lower_bound_no_match {
            args: func_args![value: value!({"tags": ["a:400"]}), query: "NOT a:[50 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_tag_lower_bound_no_match {
            args: func_args![value: value!({"tags": ["a:400"]}), query: "-a:[50 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_lower_bound_string {
            args: func_args![value: value!({"tags": ["a:400"]}), query: r#"a:["4" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_lower_bound_string {
            args: func_args![value: value!({"tags": ["a:400"]}), query: r#"NOT a:["4" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_lower_bound_string {
            args: func_args![value: value!({"tags": ["a:400"]}), query: r#"-a:["4" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_lower_bound_string_no_match {
            args: func_args![value: value!({"tags": ["a:400"]}), query: r#"a:["50" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_tag_lower_bound_string_no_match {
            args: func_args![value: value!({"tags": ["a:400"]}), query: r#"NOT a:["50" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_tag_lower_bound_string_no_match {
            args: func_args![value: value!({"tags": ["a:400"]}), query: r#"-a:["50" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_upper_bound {
            args: func_args![value: value!({"tags": ["a:300"]}), query: "a:[* TO 4]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_upper_bound {
            args: func_args![value: value!({"tags": ["a:300"]}), query: "NOT a:[* TO 4]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_upper_bound {
            args: func_args![value: value!({"tags": ["a:300"]}), query: "-a:[* TO 4]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_upper_bound_no_match {
            args: func_args![value: value!({"tags": ["a:50"]}), query: "a:[* TO 400]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_tag_upper_bound_no_match {
            args: func_args![value: value!({"tags": ["a:50"]}), query: "NOT a:[* TO 400]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_tag_upper_bound_no_match {
            args: func_args![value: value!({"tags": ["a:50"]}), query: "-a:[* TO 400]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_upper_bound_string {
            args: func_args![value: value!({"tags": ["a:300"]}), query: r#"a:[* TO "4"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_upper_bound_string {
            args: func_args![value: value!({"tags": ["a:300"]}), query: r#"NOT a:[* TO "4"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_upper_bound_string {
            args: func_args![value: value!({"tags": ["a:300"]}), query: r#"-a:[* TO "4"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_upper_bound_string_no_match {
            args: func_args![value: value!({"tags": ["a:50"]}), query: r#"a:[* TO "400"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_tag_upper_bound_string_no_match {
            args: func_args![value: value!({"tags": ["a:50"]}), query: r#"NOT a:[* TO "400"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_tag_upper_bound_string_no_match {
            args: func_args![value: value!({"tags": ["a:50"]}), query: r#"-a:[* TO "400"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_between {
            args: func_args![value: value!({"tags": ["a:500"]}), query: "a:[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_between {
            args: func_args![value: value!({"tags": ["a:500"]}), query: "NOT a:[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_between {
            args: func_args![value: value!({"tags": ["a:500"]}), query: "-a:[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_between_no_match {
            args: func_args![value: value!({"tags": ["a:70"]}), query: "a:[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_tag_between_no_match {
            args: func_args![value: value!({"tags": ["a:70"]}), query: "NOT a:[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_tag_between_no_match {
            args: func_args![value: value!({"tags": ["a:70"]}), query: "-a:[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_tag_between_string {
            args: func_args![value: value!({"tags": ["a:500"]}), query: r#"a:["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_tag_between_string {
            args: func_args![value: value!({"tags": ["a:500"]}), query: r#"NOT a:["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_tag_between_string {
            args: func_args![value: value!({"tags": ["a:500"]}), query: r#"-a:["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_tag_between_no_match_string {
            args: func_args![value: value!({"tags": ["a:70"]}), query: r#"a:["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_tag_between_no_match_string {
            args: func_args![value: value!({"tags": ["a:70"]}), query: r#"NOT a:["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_tag_between_no_match_string {
            args: func_args![value: value!({"tags": ["a:70"]}), query: r#"-a:["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_facet_unbounded {
            args: func_args![value: value!({"custom": {"a": 1}}), query: "@a:[* TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_unbounded {
            args: func_args![value: value!({"custom": {"a": 1}}), query: "NOT @a:[* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_unbounded {
            args: func_args![value: value!({"custom": {"a": 1}}), query: "-@a:[* TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_lower_bound {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "@a:[4 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_lower_bound {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "NOT @a:[4 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_lower_bound {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "-@a:[4 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_lower_bound_no_match {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "@a:[50 TO *]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_facet_lower_bound_no_match {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "NOT @a:[50 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_facet_lower_bound_no_match {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "-@a:[50 TO *]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_facet_lower_bound_string {
            args: func_args![value: value!({"custom": {"a": "5"}}), query: r#"@a:["4" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_lower_bound_string {
            args: func_args![value: value!({"custom": {"a": "5"}}), query: r#"NOT @a:["4" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_lower_bound_string {
            args: func_args![value: value!({"custom": {"a": "5"}}), query: r#"-@a:["4" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_lower_bound_string_no_match {
            args: func_args![value: value!({"custom": {"a": "400"}}), query: r#"@a:["50" TO *]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_facet_lower_bound_string_no_match {
            args: func_args![value: value!({"custom": {"a": "400"}}), query: r#"NOT @a:["50" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_facet_lower_bound_string_no_match {
            args: func_args![value: value!({"custom": {"a": "400"}}), query: r#"-@a:["50" TO *]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_facet_upper_bound {
            args: func_args![value: value!({"custom": {"a": 1}}), query: "@a:[* TO 4]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_upper_bound {
            args: func_args![value: value!({"custom": {"a": 1}}), query: "NOT @a:[* TO 4]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_upper_bound {
            args: func_args![value: value!({"custom": {"a": 1}}), query: "-@a:[* TO 4]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_upper_bound_no_match {
            args: func_args![value: value!({"custom": {"a": 500}}), query: "@a:[* TO 400]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_facet_upper_bound_no_match {
            args: func_args![value: value!({"custom": {"a": 500}}), query: "NOT @a:[* TO 400]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_facet_upper_bound_no_match {
            args: func_args![value: value!({"custom": {"a": 500}}), query: "-@a:[* TO 400]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_facet_upper_bound_string {
            args: func_args![value: value!({"custom": {"a": "3"}}), query: r#"@a:[* TO "4"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_upper_bound_string {
            args: func_args![value: value!({"custom": {"a": "3"}}), query: r#"NOT @a:[* TO "4"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_upper_bound_string {
            args: func_args![value: value!({"custom": {"a": "3"}}), query: r#"-@a:[* TO "4"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_upper_bound_string_no_match {
            args: func_args![value: value!({"custom": {"a": "5"}}), query: r#"@a:[* TO "400"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_facet_upper_bound_string_no_match {
            args: func_args![value: value!({"custom": {"a": "5"}}), query: r#"NOT @a:[* TO "400"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_facet_upper_bound_string_no_match {
            args: func_args![value: value!({"custom": {"a": "5"}}), query: r#"-@a:[* TO "400"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        range_facet_between {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "@a:[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_between {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "NOT @a:[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_between {
            args: func_args![value: value!({"custom": {"a": 5}}), query: "-@a:[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_between_no_match {
            args: func_args![value: value!({"custom": {"a": 200}}), query: "@a:[1 TO 6]"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_facet_between_no_match {
            args: func_args![value: value!({"custom": {"a": 200}}), query: "NOT @a:[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_facet_between_no_match {
            args: func_args![value: value!({"custom": {"a": 200}}), query: "-@a:[1 TO 6]"],
            want: Ok(true),
            tdef: type_def(),
        }

        range_facet_between_string {
            args: func_args![value: value!({"custom": {"a": "500"}}), query: r#"@a:["1" TO "6"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        not_range_facet_between_string {
            args: func_args![value: value!({"custom": {"a": "500"}}), query: r#"NOT @a:["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_range_facet_between_string {
            args: func_args![value: value!({"custom": {"a": "500"}}), query: r#"-@a:["1" TO "6"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        range_facet_between_no_match_string {
            args: func_args![value: value!({"custom": {"a": "7"}}), query: r#"@a:["1" TO "60"]"#],
            want: Ok(false),
            tdef: type_def(),
        }

        not_range_facet_between_no_match_string {
            args: func_args![value: value!({"custom": {"a": "7"}}), query: r#"NOT @a:["1" TO "60"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_range_facet_between_no_match_string {
            args: func_args![value: value!({"custom": {"a": "7"}}), query: r#"-@a:["1" TO "60"]"#],
            want: Ok(true),
            tdef: type_def(),
        }

        exclusive_range_message {
            args: func_args![value: value!({"message": "100"}), query: "{1 TO 2}"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_exclusive_range_message {
            args: func_args![value: value!({"message": "100"}), query: "NOT {1 TO 2}"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_exclusive_range_message {
            args: func_args![value: value!({"message": "100"}), query: "-{1 TO 2}"],
            want: Ok(false),
            tdef: type_def(),
        }

        exclusive_range_message_no_match {
            args: func_args![value: value!({"message": "1"}), query: "{1 TO 2}"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_exclusive_range_message_no_match {
            args: func_args![value: value!({"message": "1"}), query: "NOT {1 TO 2}"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_exclusive_range_message_no_match {
            args: func_args![value: value!({"message": "1"}), query: "-{1 TO 2}"],
            want: Ok(true),
            tdef: type_def(),
        }

        exclusive_range_message_lower {
            args: func_args![value: value!({"message": "200"}), query: "{1 TO *}"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_exclusive_range_message_lower {
            args: func_args![value: value!({"message": "200"}), query: "NOT {1 TO *}"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_exclusive_range_message_lower {
            args: func_args![value: value!({"message": "200"}), query: "-{1 TO *}"],
            want: Ok(false),
            tdef: type_def(),
        }

        exclusive_range_message_lower_no_match {
            args: func_args![value: value!({"message": "1"}), query: "{1 TO *}"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_exclusive_range_message_lower_no_match {
            args: func_args![value: value!({"message": "1"}), query: "NOT {1 TO *}"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_exclusive_range_message_lower_no_match {
            args: func_args![value: value!({"message": "1"}), query: "-{1 TO *}"],
            want: Ok(true),
            tdef: type_def(),
        }

        exclusive_range_message_upper {
            args: func_args![value: value!({"message": "200"}), query: "{* TO 3}"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_exclusive_range_message_upper {
            args: func_args![value: value!({"message": "200"}), query: "NOT {* TO 3}"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_exclusive_range_message_upper {
            args: func_args![value: value!({"message": "200"}), query: "-{* TO 3}"],
            want: Ok(false),
            tdef: type_def(),
        }

        exclusive_range_message_upper_no_match {
            args: func_args![value: value!({"message": "3"}), query: "{* TO 3}"],
            want: Ok(false),
            tdef: type_def(),
        }

        not_exclusive_range_message_upper_no_match {
            args: func_args![value: value!({"message": "3"}), query: "NOT {* TO 3}"],
            want: Ok(true),
            tdef: type_def(),
        }

        negate_exclusive_range_message_upper_no_match {
            args: func_args![value: value!({"message": "3"}), query: "-{* TO 3}"],
            want: Ok(true),
            tdef: type_def(),
        }

        message_and {
            args: func_args![value: value!({"message": "this contains that"}), query: "this AND that"],
            want: Ok(true),
            tdef: type_def(),
        }

        message_and_not {
            args: func_args![value: value!({"message": "this contains that"}), query: "this AND NOT that"],
            want: Ok(false),
            tdef: type_def(),
        }

        message_or {
            args: func_args![value: value!({"message": "only contains that"}), query: "this OR that"],
            want: Ok(true),
            tdef: type_def(),
        }

        message_or_not {
            args: func_args![value: value!({"message": "only contains that"}), query: "this OR NOT that"],
            want: Ok(false),
            tdef: type_def(),
        }

        message_and_or {
            args: func_args![value: value!({"message": "this contains that"}), query: "this AND (that OR the_other)"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_message_and_or {
            args: func_args![value: value!({"message": "this contains that"}), query: "this AND NOT (that OR the_other)"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_message_and_or {
            args: func_args![value: value!({"message": "this contains that"}), query: "this AND -(that OR the_other)"],
            want: Ok(false),
            tdef: type_def(),
        }

        message_and_or_2 {
            args: func_args![value: value!({"message": "this contains the_other"}), query: "this AND (that OR the_other)"],
            want: Ok(true),
            tdef: type_def(),
        }

        not_message_and_or_2 {
            args: func_args![value: value!({"message": "this contains the_other"}), query: "this AND NOT (that OR the_other)"],
            want: Ok(false),
            tdef: type_def(),
        }

        negate_message_and_or_2 {
            args: func_args![value: value!({"message": "this contains the_other"}), query: "this AND -(that OR the_other)"],
            want: Ok(false),
            tdef: type_def(),
        }

        message_or_and {
            args: func_args![value: value!({"message": "just this"}), query: "this OR (that AND the_other)"],
            want: Ok(true),
            tdef: type_def(),
        }

        message_or_and_no_match {
            args: func_args![value: value!({"message": "that and nothing else"}), query: "this OR (that AND the_other)"],
            want: Ok(false),
            tdef: type_def(),
        }

        message_or_and_2 {
            args: func_args![value: value!({"message": "that plus the_other"}), query: "this OR (that AND the_other)"],
            want: Ok(true),
            tdef: type_def(),
        }

        message_or_and_2_no_match {
            args: func_args![value: value!({"message": "nothing plus the_other"}), query: "this OR (that AND the_other)"],
            want: Ok(false),
            tdef: type_def(),
        }

        kitchen_sink {
            args: func_args![value: value!({"host": "this"}), query: "host:this OR ((@b:test* AND c:that) AND d:the_other @e:[1 TO 5])"],
            want: Ok(true),
            tdef: type_def(),
        }

        kitchen_sink_2 {
            args: func_args![value: value!({"tags": ["c:that", "d:the_other"], "custom": {"b": "testing", "e": 3}}), query: "host:this OR ((@b:test* AND c:that) AND d:the_other @e:[1 TO 5])"],
            want: Ok(true),
            tdef: type_def(),
        }

        integer_range_float_value_match {
            args: func_args![value: value!({"custom": {"level": 7.0}}), query: "@level:[7 TO 10]"],
            want: Ok(true),
            tdef: type_def(),
        }

        integer_range_float_value_no_match {
            args: func_args![value: value!({"custom": {"level": 6.9}}), query: "@level:[7 TO 10]"],
            want: Ok(false),
            tdef: type_def(),
        }

        float_range_integer_value_match {
            args: func_args![value: value!({"custom": {"level": 7}}), query: "@level:[7.0 TO 10.0]"],
            want: Ok(true),
            tdef: type_def(),
        }

        float_range_integer_value_no_match {
            args: func_args![value: value!({"custom": {"level": 6}}), query: "@level:[7.0 TO 10.0]"],
            want: Ok(false),
            tdef: type_def(),
        }
    ];
}
