use std::borrow::Cow;

use bytes::Bytes;
use datadog_filter::{
    regex::{wildcard_regex, word_regex},
    Filter, Matcher, Resolver, Run,
};
use datadog_search_syntax::{Comparison, ComparisonValue, Field};
use vector_core::event::{LogEvent, Value};

#[derive(Default, Clone)]
pub struct EventFilter;

/// Uses the default `Resolver`, to build a `Vec<Field>`.
impl Resolver for EventFilter {}

impl Filter<LogEvent> for EventFilter {
    fn exists(&self, field: Field) -> Box<dyn Matcher<LogEvent>> {
        match field {
            Field::Tag(tag) => {
                let starts_with = format!("{}:", tag);

                any_string_match("tags", move |value| {
                    value == tag || value.starts_with(&starts_with)
                })
            }
            // Literal field 'tags' needs to be compared by key.
            Field::Reserved(field) if field == "tags" => {
                any_string_match("tags", move |value| value == field)
            }
            Field::Default(f) | Field::Facet(f) | Field::Reserved(f) => {
                Run::boxed(move |log: &LogEvent| log.get(&f).is_some())
            }
        }
    }

    fn equals(&self, field: Field, to_match: &str) -> Box<dyn Matcher<LogEvent>> {
        match field {
            // Default fields are compared by word boundary.
            Field::Default(field) => {
                let re = word_regex(to_match);

                string_match(&field, move |value| re.is_match(&value))
            }
            // A literal "tags" field should match by key.
            Field::Reserved(field) if field == "tags" => {
                let to_match = to_match.to_owned();

                array_match(field, move |values| {
                    values.contains(&Value::Bytes(Bytes::copy_from_slice(to_match.as_bytes())))
                })
            }
            // Individual tags are compared by element key:value.
            Field::Tag(tag) => {
                let value_bytes = Value::Bytes(format!("{}:{}", tag, to_match).into());

                array_match("tags", move |values| values.contains(&value_bytes))
            }
            // Everything else is matched by string equality.
            Field::Reserved(field) | Field::Facet(field) => {
                let to_match = to_match.to_owned();

                string_match(field, move |value| value == to_match)
            }
        }
    }

    fn prefix(&self, field: Field, prefix: &str) -> Box<dyn Matcher<LogEvent>> {
        match field {
            // Default fields are matched by word boundary.
            Field::Default(field) => {
                let re = word_regex(&format!("{}*", prefix));

                string_match(field, move |value| re.is_match(&value))
            }
            // Tags are recursed until a match is found.
            Field::Tag(tag) => {
                let starts_with = format!("{}:{}", tag, prefix);

                any_string_match("tags", move |value| value.starts_with(&starts_with))
            }
            // All other field types are compared by complete value.
            Field::Reserved(field) | Field::Facet(field) => {
                let prefix = prefix.to_owned();

                string_match(field, move |value| value.starts_with(&prefix))
            }
        }
    }

    fn wildcard(&self, field: Field, wildcard: &str) -> Box<dyn Matcher<LogEvent>> {
        match field {
            Field::Default(field) => {
                let re = word_regex(wildcard);

                string_match(field, move |value| re.is_match(&value))
            }
            Field::Tag(tag) => {
                let re = wildcard_regex(&format!("{}:{}", tag, wildcard));

                any_string_match("tags", move |value| re.is_match(&value))
            }
            Field::Reserved(field) | Field::Facet(field) => {
                let re = wildcard_regex(wildcard);

                string_match(field, move |value| re.is_match(&value))
            }
        }
    }

    fn compare(
        &self,
        field: Field,
        comparator: Comparison,
        comparison_value: ComparisonValue,
    ) -> Box<dyn Matcher<LogEvent>> {
        let rhs = Cow::from(comparison_value.to_string());

        match field {
            // Facets are compared numerically if the value is numeric, or as strings otherwise.
            Field::Facet(f) => {
                Run::boxed(
                    move |log: &LogEvent| match (log.get(&f), &comparison_value) {
                        // Integers.
                        (Some(Value::Integer(lhs)), ComparisonValue::Integer(rhs)) => {
                            match comparator {
                                Comparison::Lt => lhs < rhs,
                                Comparison::Lte => lhs <= rhs,
                                Comparison::Gt => lhs > rhs,
                                Comparison::Gte => lhs >= rhs,
                            }
                        }
                        // Integer value - Float boundary
                        (Some(Value::Integer(lhs)), ComparisonValue::Float(rhs)) => {
                            match comparator {
                                Comparison::Lt => (*lhs as f64) < *rhs,
                                Comparison::Lte => *lhs as f64 <= *rhs,
                                Comparison::Gt => *lhs as f64 > *rhs,
                                Comparison::Gte => *lhs as f64 >= *rhs,
                            }
                        }
                        // Floats.
                        (Some(Value::Float(lhs)), ComparisonValue::Float(rhs)) => {
                            match comparator {
                                Comparison::Lt => lhs < rhs,
                                Comparison::Lte => lhs <= rhs,
                                Comparison::Gt => lhs > rhs,
                                Comparison::Gte => lhs >= rhs,
                            }
                        }
                        // Float value - Integer boundary
                        (Some(Value::Float(lhs)), ComparisonValue::Integer(rhs)) => {
                            match comparator {
                                Comparison::Lt => *lhs < *rhs as f64,
                                Comparison::Lte => *lhs <= *rhs as f64,
                                Comparison::Gt => *lhs > *rhs as f64,
                                Comparison::Gte => *lhs >= *rhs as f64,
                            }
                        }
                        // Where the rhs is a string ref, the lhs is coerced into a string.
                        (Some(Value::Bytes(v)), ComparisonValue::String(rhs)) => {
                            let lhs = String::from_utf8_lossy(v);
                            let rhs = Cow::from(rhs);

                            match comparator {
                                Comparison::Lt => lhs < rhs,
                                Comparison::Lte => lhs <= rhs,
                                Comparison::Gt => lhs > rhs,
                                Comparison::Gte => lhs >= rhs,
                            }
                        }
                        // Otherwise, compare directly as strings.
                        (Some(Value::Bytes(v)), _) => {
                            let lhs = String::from_utf8_lossy(v);

                            match comparator {
                                Comparison::Lt => lhs < rhs,
                                Comparison::Lte => lhs <= rhs,
                                Comparison::Gt => lhs > rhs,
                                Comparison::Gte => lhs >= rhs,
                            }
                        }
                        _ => false,
                    },
                )
            }
            // Tag values need extracting by "key:value" to be compared.
            Field::Tag(tag) => any_string_match("tags", move |value| match value.split_once(":") {
                Some((t, lhs)) if t == tag => {
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
            // All other tag types are compared by string.
            Field::Default(field) | Field::Reserved(field) => {
                string_match(field, move |lhs| match comparator {
                    Comparison::Lt => lhs < rhs,
                    Comparison::Lte => lhs <= rhs,
                    Comparison::Gt => lhs > rhs,
                    Comparison::Gte => lhs >= rhs,
                })
            }
        }
    }
}

/// Returns a `Matcher` that returns true if the log event resolves to a string which
/// matches the provided `func`.
fn string_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(Cow<str>) -> bool + Send + Sync + Clone + 'static,
{
    let field = field.into();

    Run::boxed(move |log: &LogEvent| match log.get(&field) {
        Some(Value::Bytes(v)) => func(String::from_utf8_lossy(v)),
        _ => false,
    })
}

/// Returns a `Matcher` that returns true if the log event resolves to an array, where
/// the vector of `Value`s the array contains matches the provided `func`.
fn array_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(&Vec<Value>) -> bool + Send + Sync + Clone + 'static,
{
    let field = field.into();

    Run::boxed(move |log: &LogEvent| match log.get(&field) {
        Some(Value::Array(values)) => func(values),
        _ => false,
    })
}

/// Returns a `Matcher` that returns true if the log event resolves to an array, where
/// at least one `Value` it contains matches the provided `func`.
fn any_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(&Value) -> bool + Send + Sync + Clone + 'static,
{
    array_match(field, move |values| values.iter().any(&func))
}

/// Retrns a `Matcher` that returns true if the log event resolves to an array of strings,
/// where at least one string matches the provided `func`.
fn any_string_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(Cow<str>) -> bool + Send + Sync + Clone + 'static,
{
    any_match(field, move |value| {
        let bytes = value.as_bytes();
        func(String::from_utf8_lossy(&bytes))
    })
}
