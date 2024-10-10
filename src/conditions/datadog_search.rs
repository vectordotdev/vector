use std::{borrow::Cow, str::FromStr};
use vrl::path::PathParseError;

use bytes::Bytes;
use vector_lib::configurable::configurable_component;
use vector_lib::event::{Event, LogEvent, Value};
use vrl::datadog_filter::regex::{wildcard_regex, word_regex};
use vrl::datadog_filter::{build_matcher, Filter, Matcher, Resolver, Run};
use vrl::datadog_search_syntax::{Comparison, ComparisonValue, Field, QueryNode};

use super::{Condition, Conditional, ConditionalConfig};

/// A condition that uses the [Datadog Search](https://docs.datadoghq.com/logs/explorer/search_syntax/) query syntax against an event.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
pub struct DatadogSearchConfig {
    /// The query string.
    source: QueryNode,
}

impl Default for DatadogSearchConfig {
    fn default() -> Self {
        Self {
            source: QueryNode::MatchAllDocs,
        }
    }
}

impl FromStr for DatadogSearchConfig {
    type Err = <QueryNode as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(|source| Self { source })
    }
}

impl From<QueryNode> for DatadogSearchConfig {
    fn from(source: QueryNode) -> Self {
        Self { source }
    }
}

impl_generate_config_from_default!(DatadogSearchConfig);

/// Runner that contains the boxed `Matcher` function to check whether an `Event` matches
/// a [Datadog Search Syntax query](https://docs.datadoghq.com/logs/explorer/search_syntax/).
#[derive(Debug, Clone)]
pub struct DatadogSearchRunner {
    matcher: Box<dyn Matcher<Event>>,
}

impl Conditional for DatadogSearchRunner {
    fn check(&self, e: Event) -> (bool, Event) {
        let result = self.matcher.run(&e);
        (result, e)
    }
}

impl ConditionalConfig for DatadogSearchConfig {
    fn build(
        &self,
        _enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<Condition> {
        let matcher = as_log(build_matcher(&self.source, &EventFilter).map_err(|e| e.to_string())?);

        Ok(Condition::DatadogSearch(DatadogSearchRunner { matcher }))
    }
}

/// Run the provided `Matcher` when we're dealing with `LogEvent`s. Otherwise, return false.
fn as_log(matcher: Box<dyn Matcher<LogEvent>>) -> Box<dyn Matcher<Event>> {
    Run::boxed(move |ev| match ev {
        Event::Log(log) => matcher.run(log),
        _ => false,
    })
}

#[derive(Default, Clone)]
struct EventFilter;

/// Uses the default `Resolver`, to build a `Vec<Field>`.
impl Resolver for EventFilter {}

impl Filter<LogEvent> for EventFilter {
    fn exists(&self, field: Field) -> Result<Box<dyn Matcher<LogEvent>>, PathParseError> {
        Ok(match field {
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
            Field::Default(f) | Field::Attribute(f) | Field::Reserved(f) => {
                Run::boxed(move |log: &LogEvent| {
                    log.parse_path_and_get_value(f.as_str())
                        .ok()
                        .flatten()
                        .is_some()
                })
            }
        })
    }

    fn equals(
        &self,
        field: Field,
        to_match: &str,
    ) -> Result<Box<dyn Matcher<LogEvent>>, PathParseError> {
        Ok(match field {
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
            // Reserved values are matched by string equality.
            Field::Reserved(field) => {
                let to_match = to_match.to_owned();

                string_match(field, move |value| value == to_match)
            }
            // Attribute values can be strings or numeric types
            Field::Attribute(field) => {
                let to_match = to_match.to_owned();

                string_or_numeric_match(field, move |value| value == to_match)
            }
        })
    }

    fn prefix(
        &self,
        field: Field,
        prefix: &str,
    ) -> Result<Box<dyn Matcher<LogEvent>>, PathParseError> {
        Ok(match field {
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
            Field::Reserved(field) | Field::Attribute(field) => {
                let prefix = prefix.to_owned();

                string_match(field, move |value| value.starts_with(&prefix))
            }
        })
    }

    fn wildcard(
        &self,
        field: Field,
        wildcard: &str,
    ) -> Result<Box<dyn Matcher<LogEvent>>, PathParseError> {
        Ok(match field {
            Field::Default(field) => {
                let re = word_regex(wildcard);

                string_match(field, move |value| re.is_match(&value))
            }
            Field::Tag(tag) => {
                let re = wildcard_regex(&format!("{}:{}", tag, wildcard));

                any_string_match("tags", move |value| re.is_match(&value))
            }
            Field::Reserved(field) | Field::Attribute(field) => {
                let re = wildcard_regex(wildcard);

                string_match(field, move |value| re.is_match(&value))
            }
        })
    }

    fn compare(
        &self,
        field: Field,
        comparator: Comparison,
        comparison_value: ComparisonValue,
    ) -> Result<Box<dyn Matcher<LogEvent>>, PathParseError> {
        let rhs = Cow::from(comparison_value.to_string());

        Ok(match field {
            // Attributes are compared numerically if the value is numeric, or as strings otherwise.
            Field::Attribute(f) => {
                Run::boxed(move |log: &LogEvent| {
                    match (
                        log.parse_path_and_get_value(f.as_str()).ok().flatten(),
                        &comparison_value,
                    ) {
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
                                Comparison::Lt => lhs.into_inner() < *rhs,
                                Comparison::Lte => lhs.into_inner() <= *rhs,
                                Comparison::Gt => lhs.into_inner() > *rhs,
                                Comparison::Gte => lhs.into_inner() >= *rhs,
                            }
                        }
                        // Float value - Integer boundary
                        (Some(Value::Float(lhs)), ComparisonValue::Integer(rhs)) => {
                            match comparator {
                                Comparison::Lt => lhs.into_inner() < *rhs as f64,
                                Comparison::Lte => lhs.into_inner() <= *rhs as f64,
                                Comparison::Gt => lhs.into_inner() > *rhs as f64,
                                Comparison::Gte => lhs.into_inner() >= *rhs as f64,
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
                    }
                })
            }
            // Tag values need extracting by "key:value" to be compared.
            Field::Tag(tag) => any_string_match("tags", move |value| match value.split_once(':') {
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
        })
    }
}

/// Returns a `Matcher` that returns true if the log event resolves to a string or
/// numeric which matches the provided `func`.
fn string_or_numeric_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(Cow<str>) -> bool + Send + Sync + Clone + 'static,
{
    let field = field.into();

    Run::boxed(move |log: &LogEvent| {
        match log.parse_path_and_get_value(field.as_str()).ok().flatten() {
            Some(Value::Bytes(v)) => func(String::from_utf8_lossy(v)),
            Some(Value::Integer(v)) => func(v.to_string().into()),
            Some(Value::Float(v)) => func(v.to_string().into()),
            _ => false,
        }
    })
}

/// Returns a `Matcher` that returns true if the log event resolves to a string which
/// matches the provided `func`.
fn string_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(Cow<str>) -> bool + Send + Sync + Clone + 'static,
{
    let field = field.into();

    Run::boxed(move |log: &LogEvent| {
        match log.parse_path_and_get_value(field.as_str()).ok().flatten() {
            Some(Value::Bytes(v)) => func(String::from_utf8_lossy(v)),
            _ => false,
        }
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

    Run::boxed(move |log: &LogEvent| {
        match log.parse_path_and_get_value(field.as_str()).ok().flatten() {
            Some(Value::Array(values)) => func(values),
            _ => false,
        }
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

/// Returns a `Matcher` that returns true if the log event resolves to an array of strings,
/// where at least one string matches the provided `func`.
fn any_string_match<S, F>(field: S, func: F) -> Box<dyn Matcher<LogEvent>>
where
    S: Into<String>,
    F: Fn(Cow<str>) -> bool + Send + Sync + Clone + 'static,
{
    any_match(field, move |value| {
        let bytes = value.coerce_to_bytes();
        func(String::from_utf8_lossy(&bytes))
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::log_event;

    /// Returns the following: Datadog Search Syntax source (to be parsed), an `Event` that
    /// should pass when matched against the compiled source, and an `Event` that should fail.
    /// This is exported as public so any implementor of this lib can assert that each check
    /// still passes/fails in the context it's used.
    fn get_checks() -> Vec<(&'static str, Event, Event)> {
        vec![
            // Tag exists.
            (
                "_exists_:a",                        // Source
                log_event!["tags" => vec!["a:foo"]], // Pass
                log_event!["tags" => vec!["b:foo"]], // Fail
            ),
            // Tag exists with - in name.
            (
                "_exists_:a-b",                        // Source
                log_event!["tags" => vec!["a-b:foo"]], // Pass
                log_event!["tags" => vec!["ab:foo"]],  // Fail
            ),
            // Tag exists (negate).
            (
                "NOT _exists_:a",
                log_event!["tags" => vec!["b:foo"]],
                log_event!("tags" => vec!["a:foo"]),
            ),
            // Tag exists (negate w/-).
            (
                "-_exists_:a",
                log_event!["tags" => vec!["b:foo"]],
                log_event!["tags" => vec!["a:foo"]],
            ),
            // Attribute exists.
            (
                "_exists_:@b",
                log_event!["b" => "foo"],
                log_event!["a" => "foo"],
            ),
            // Attribute with - in name, exists.
            // TODO: this is a test case which exists in the Datadog implementation of the feature.
            //       in our implementation, it fails because parse_path_and_get_value, indicates that
            //       the `-` in the field name is an invalid field name.
            // (
            //     "_exists_:@foo-bar",
            //     log_event!["foo-bar" => "foo"],
            //     log_event!["foobar" => "foo"],
            // ),
            // Attribute exists (negate).
            (
                "NOT _exists_:@b",
                log_event!["a" => "foo"],
                log_event!["b" => "foo"],
            ),
            // Attribute exists (negate w/-).
            (
                "-_exists_:@b",
                log_event!["a" => "foo"],
                log_event!["b" => "foo"],
            ),
            // Tag doesn't exist.
            (
                "_missing_:a",
                log_event![],
                log_event!["tags" => vec!["a:foo"]],
            ),
            // Tag doesn't exist (negate).
            (
                "NOT _missing_:a",
                log_event!["tags" => vec!["a:foo"]],
                log_event![],
            ),
            // Tag doesn't exist (negate w/-).
            (
                "-_missing_:a",
                log_event!["tags" => vec!["a:foo"]],
                log_event![],
            ),
            // Attribute doesn't exist.
            (
                "_missing_:@b",
                log_event!["a" => "foo"],
                log_event!["b" => "foo"],
            ),
            // Attribute doesn't exist (negate).
            (
                "NOT _missing_:@b",
                log_event!["b" => "foo"],
                log_event!["a" => "foo"],
            ),
            // Attribute doesn't exist (negate w/-).
            (
                "-_missing_:@b",
                log_event!["b" => "foo"],
                log_event!["a" => "foo"],
            ),
            // Keyword.
            ("bla", log_event!["message" => "bla"], log_event![]),
            (
                "foo",
                log_event!["message" => r#"{"key": "foo"}"#],
                log_event![],
            ),
            (
                "bar",
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
                log_event![],
            ),
            // Keyword (negate).
            (
                "NOT bla",
                log_event!["message" => "nothing"],
                log_event!["message" => "bla"],
            ),
            (
                "NOT foo",
                log_event![],
                log_event!["message" => r#"{"key": "foo"}"#],
            ),
            (
                "NOT bar",
                log_event![],
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
            ),
            // Keyword (negate w/-).
            (
                "-bla",
                log_event!["message" => "nothing"],
                log_event!["message" => "bla"],
            ),
            (
                "-foo",
                log_event![],
                log_event!["message" => r#"{"key": "foo"}"#],
            ),
            (
                "-bar",
                log_event![],
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
            ),
            // Quoted keyword.
            (r#""bla""#, log_event!["message" => "bla"], log_event![]),
            (
                r#""foo""#,
                log_event!["message" => r#"{"key": "foo"}"#],
                log_event![],
            ),
            (
                r#""bar""#,
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
                log_event![],
            ),
            // Quoted keyword (negate).
            (r#"NOT "bla""#, log_event![], log_event!["message" => "bla"]),
            (
                r#"NOT "foo""#,
                log_event![],
                log_event!["message" => r#"{"key": "foo"}"#],
            ),
            (
                r#"NOT "bar""#,
                log_event![],
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
            ),
            // Quoted keyword (negate w/-).
            (r#"-"bla""#, log_event![], log_event!["message" => "bla"]),
            (
                r#"NOT "foo""#,
                log_event![],
                log_event!["message" => r#"{"key": "foo"}"#],
            ),
            (
                r#"NOT "bar""#,
                log_event![],
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
            ),
            // Tag match.
            (
                "a:bla",
                log_event!["tags" => vec!["a:bla"]],
                log_event!["tags" => vec!["b:bla"]],
            ),
            // Reserved tag match.
            (
                "host:foo",
                log_event!["host" => "foo"],
                log_event!["tags" => vec!["host:foo"]],
            ),
            (
                "host:foo",
                log_event!["host" => "foo"],
                log_event!["host" => "foobar"],
            ),
            (
                "host:foo",
                log_event!["host" => "foo"],
                log_event!["host" => r#"{"value": "foo"}"#],
            ),
            // Tag match (negate).
            (
                "NOT a:bla",
                log_event!["tags" => vec!["b:bla"]],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Reserved tag match (negate).
            (
                "NOT host:foo",
                log_event!["tags" => vec!["host:fo  o"]],
                log_event!["host" => "foo"],
            ),
            // Tag match (negate w/-).
            (
                "-a:bla",
                log_event!["tags" => vec!["b:bla"]],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Reserved tag match (negate w/-).
            (
                "-trace_id:foo",
                log_event![],
                log_event!["trace_id" => "foo"],
            ),
            // Quoted tag match.
            (
                r#"a:"bla""#,
                log_event!["tags" => vec!["a:bla"]],
                log_event!["a" => "bla"],
            ),
            // Quoted tag match (negate).
            (
                r#"NOT a:"bla""#,
                log_event!["a" => "bla"],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Quoted tag match (negate w/-).
            (
                r#"-a:"bla""#,
                log_event!["a" => "bla"],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // String attribute match.
            (
                "@a:bla",
                log_event!["a" => "bla"],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // String attribute match (negate).
            (
                "NOT @a:bla",
                log_event!["tags" => vec!["a:bla"]],
                log_event!["a" => "bla"],
            ),
            // String attribute match single character.
            ("@a:b", log_event!["a" => "b"], log_event!["a" => "c"]),
            // String attribute match special chars
            (
                "@a:va\\/lue",
                log_event!["a" => "va/lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match escaped && chars
            (
                "@a:va\\&&lue",
                log_event!["a" => "va&&lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match escaped spaces
            (
                "@a:va\\ lue",
                log_event!["a" => "va lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match escaped || chars
            (
                "@a:va\\||lue",
                log_event!["a" => "va||lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match escaped () chars
            (
                "@a:va\\(lue",
                log_event!["a" => "va(lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match escaped * chars
            (
                "@a:va\\*lue",
                log_event!["a" => "va*lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match ~ chars
            // TODO: in Datadog, this character does not need to be escaped.
            (
                "@a:va\\~lue",
                log_event!["a" => "va~lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match ^ chars
            // TODO: in Datadog, this character does not need to be escaped.
            (
                "@a:va\\^lue",
                log_event!["a" => "va^lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match / chars
            (
                "@a:va/lue",
                log_event!["a" => "va/lue"],
                log_event!["a" => "value"],
            ),
            // String attribute match (negate w/-).
            (
                "-@a:bla",
                log_event!["tags" => vec!["a:bla"]],
                log_event!["a" => "bla"],
            ),
            // Quoted attribute match.
            (
                r#"@a:"bla""#,
                log_event!["a" => "bla"],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Quoted attribute match (negate).
            (
                r#"NOT @a:"bla""#,
                log_event!["tags" => vec!["a:bla"]],
                log_event!["a" => "bla"],
            ),
            // Quoted attribute match (negate w/-).
            (
                r#"-@a:"bla""#,
                log_event!["tags" => vec!["a:bla"]],
                log_event!["a" => "bla"],
            ),
            // Integer attribute match.
            (
                "@a:200",
                log_event!["a" => 200],
                log_event!["tags" => vec!["a:200"]],
            ),
            // Integer attribute match (negate w/-).
            ("-@a:200", log_event!["a" => 199], log_event!["a" => 200]),
            // Float attribute match.
            (
                "@a:0.75",
                log_event!["a" => 0.75],
                log_event!["tags" => vec!["a:0.75"]],
            ),
            // Float attribute match (negate w/-).
            ("-@a:0.75", log_event!["a" => 0.74], log_event!["a" => 0.75]),
            // Wildcard prefix.
            (
                "*bla",
                log_event!["message" => "foobla"],
                log_event!["message" => "blafoo"],
            ),
            // Wildcard prefix (negate).
            (
                "NOT *bla",
                log_event!["message" => "blafoo"],
                log_event!["message" => "foobla"],
            ),
            // Wildcard prefix (negate w/-).
            (
                "-*bla",
                log_event!["message" => "blafoo"],
                log_event!["message" => "foobla"],
            ),
            // Wildcard suffix.
            (
                "bla*",
                log_event!["message" => "blafoo"],
                log_event!["message" => "foobla"],
            ),
            // Wildcard suffix (negate).
            (
                "NOT bla*",
                log_event!["message" => "foobla"],
                log_event!["message" => "blafoo"],
            ),
            // Wildcard suffix (negate w/-).
            (
                "-bla*",
                log_event!["message" => "foobla"],
                log_event!["message" => "blafoo"],
            ),
            // Multiple wildcards.
            ("*b*la*", log_event!["message" => "foobla"], log_event![]),
            // Multiple wildcards (negate).
            (
                "NOT *b*la*",
                log_event![],
                log_event!["message" => "foobla"],
            ),
            // Multiple wildcards (negate w/-).
            ("-*b*la*", log_event![], log_event!["message" => "foobla"]),
            // Wildcard prefix - tag.
            (
                "a:*bla",
                log_event!["tags" => vec!["a:foobla"]],
                log_event!["tags" => vec!["a:blafoo"]],
            ),
            // Wildcard prefix - tag (negate).
            (
                "NOT a:*bla",
                log_event!["tags" => vec!["a:blafoo"]],
                log_event!["tags" => vec!["a:foobla"]],
            ),
            // Wildcard prefix - tag (negate w/-).
            (
                "-a:*bla",
                log_event!["tags" => vec!["a:blafoo"]],
                log_event!["tags" => vec!["a:foobla"]],
            ),
            // Wildcard suffix - tag.
            (
                "b:bla*",
                log_event!["tags" => vec!["b:blabop"]],
                log_event!["tags" => vec!["b:bopbla"]],
            ),
            // Wildcard suffix - tag (negate).
            (
                "NOT b:bla*",
                log_event!["tags" => vec!["b:bopbla"]],
                log_event!["tags" => vec!["b:blabop"]],
            ),
            // Wildcard suffix - tag (negate w/-).
            (
                "-b:bla*",
                log_event!["tags" => vec!["b:bopbla"]],
                log_event!["tags" => vec!["b:blabop"]],
            ),
            // Multiple wildcards - tag.
            (
                "c:*b*la*",
                log_event!["tags" => vec!["c:foobla"]],
                log_event!["custom" => r#"{"title" => "foobla"}"#],
            ),
            // Multiple wildcards - tag (negate).
            (
                "NOT c:*b*la*",
                log_event!["custom" => r#"{"title" => "foobla"}"#],
                log_event!["tags" => vec!["c:foobla"]],
            ),
            // Multiple wildcards - tag (negate w/-).
            (
                "-c:*b*la*",
                log_event!["custom" => r#"{"title" => "foobla"}"#],
                log_event!["tags" => vec!["c:foobla"]],
            ),
            // Wildcard prefix - attribute.
            (
                "@a:*bla",
                log_event!["a" => "foobla"],
                log_event!["tags" => vec!["a:foobla"]],
            ),
            // Wildcard prefix - attribute (negate).
            (
                "NOT @a:*bla",
                log_event!["tags" => vec!["a:foobla"]],
                log_event!["a" => "foobla"],
            ),
            // Wildcard prefix - attribute (negate w/-).
            (
                "-@a:*bla",
                log_event!["tags" => vec!["a:foobla"]],
                log_event!["a" => "foobla"],
            ),
            // Wildcard suffix - attribute.
            (
                "@b:bla*",
                log_event!["b" => "blabop"],
                log_event!["tags" => vec!["b:blabop"]],
            ),
            // Wildcard suffix - attribute (negate).
            (
                "NOT @b:bla*",
                log_event!["tags" => vec!["b:blabop"]],
                log_event!["b" => "blabop"],
            ),
            // Wildcard suffix - attribute (negate w/-).
            (
                "-@b:bla*",
                log_event!["tags" => vec!["b:blabop"]],
                log_event!["b" => "blabop"],
            ),
            // Multiple wildcards - attribute.
            (
                "@c:*b*la*",
                log_event!["c" => "foobla"],
                log_event!["tags" => vec!["c:foobla"]],
            ),
            // Multiple wildcards - attribute (negate).
            (
                "NOT @c:*b*la*",
                log_event!["tags" => vec!["c:foobla"]],
                log_event!["c" => "foobla"],
            ),
            // Multiple wildcards - attribute (negate w/-).
            (
                "-@c:*b*la*",
                log_event!["tags" => vec!["c:foobla"]],
                log_event!["c" => "foobla"],
            ),
            // Special case for tags.
            (
                "tags:a",
                log_event!["tags" => vec!["a", "b", "c"]],
                log_event!["tags" => vec!["d", "e", "f"]],
            ),
            // Special case for tags (negate).
            (
                "NOT tags:a",
                log_event!["tags" => vec!["d", "e", "f"]],
                log_event!["tags" => vec!["a", "b", "c"]],
            ),
            // Special case for tags (negate w/-).
            (
                "-tags:a",
                log_event!["tags" => vec!["d", "e", "f"]],
                log_event!["tags" => vec!["a", "b", "c"]],
            ),
            // Range - numeric, inclusive.
            (
                "[1 TO 10]",
                log_event!["message" => "1"],
                log_event!["message" => "2"],
            ),
            // Range - numeric, inclusive (negate).
            (
                "NOT [1 TO 10]",
                log_event!["message" => "2"],
                log_event!["message" => "1"],
            ),
            // Range - numeric, inclusive (negate w/-).
            (
                "-[1 TO 10]",
                log_event!["message" => "2"],
                log_event!["message" => "1"],
            ),
            // Range - numeric, inclusive, unbounded (upper).
            (
                "[50 TO *]",
                log_event!["message" => "6"],
                log_event!["message" => "40"],
            ),
            // Range - numeric, inclusive, unbounded (upper) (negate).
            (
                "NOT [50 TO *]",
                log_event!["message" => "40"],
                log_event!["message" => "6"],
            ),
            // Range - numeric, inclusive, unbounded (upper) (negate w/-).
            (
                "-[50 TO *]",
                log_event!["message" => "40"],
                log_event!["message" => "6"],
            ),
            // Range - numeric, inclusive, unbounded (lower).
            (
                "[* TO 50]",
                log_event!["message" => "3"],
                log_event!["message" => "6"],
            ),
            // Range - numeric, inclusive, unbounded (lower) (negate).
            (
                "NOT [* TO 50]",
                log_event!["message" => "6"],
                log_event!["message" => "3"],
            ),
            // Range - numeric, inclusive, unbounded (lower) (negate w/-).
            (
                "-[* TO 50]",
                log_event!["message" => "6"],
                log_event!["message" => "3"],
            ),
            // Range - numeric, inclusive, unbounded (both).
            ("[* TO *]", log_event!["message" => "foo"], log_event![]),
            // Range - numeric, inclusive, unbounded (both) (negate).
            ("NOT [* TO *]", log_event![], log_event!["message" => "foo"]),
            // Range - numeric, inclusive, unbounded (both) (negate w/-i).
            ("-[* TO *]", log_event![], log_event!["message" => "foo"]),
            // Range - numeric, inclusive, tag.
            (
                "a:[1 TO 10]",
                log_event!["tags" => vec!["a:1"]],
                log_event!["tags" => vec!["a:2"]],
            ),
            // Range - numeric, inclusive, tag (negate).
            (
                "NOT a:[1 TO 10]",
                log_event!["tags" => vec!["a:2"]],
                log_event!["tags" => vec!["a:1"]],
            ),
            // Range - numeric, inclusive, tag (negate w/-).
            (
                "-a:[1 TO 10]",
                log_event!["tags" => vec!["a:2"]],
                log_event!["tags" => vec!["a:1"]],
            ),
            // Range - numeric, inclusive, unbounded (upper), tag.
            (
                "a:[50 TO *]",
                log_event!["tags" => vec!["a:6"]],
                log_event!["tags" => vec!["a:40"]],
            ),
            // Range - numeric, inclusive, unbounded (upper), tag (negate).
            (
                "NOT a:[50 TO *]",
                log_event!["tags" => vec!["a:40"]],
                log_event!["tags" => vec!["a:6"]],
            ),
            // Range - numeric, inclusive, unbounded (upper), tag (negate w/-).
            (
                "-a:[50 TO *]",
                log_event!["tags" => vec!["a:40"]],
                log_event!["tags" => vec!["a:6"]],
            ),
            // Range - numeric, inclusive, unbounded (lower), tag.
            (
                "a:[* TO 50]",
                log_event!["tags" => vec!["a:400"]],
                log_event!["tags" => vec!["a:600"]],
            ),
            // Range - numeric, inclusive, unbounded (lower), tag (negate).
            (
                "NOT a:[* TO 50]",
                log_event!["tags" => vec!["a:600"]],
                log_event!["tags" => vec!["a:400"]],
            ),
            // Range - numeric, inclusive, unbounded (lower), tag (negate w/-).
            (
                "-a:[* TO 50]",
                log_event!["tags" => vec!["a:600"]],
                log_event!["tags" => vec!["a:400"]],
            ),
            // Range - numeric, inclusive, unbounded (both), tag.
            (
                "a:[* TO *]",
                log_event!["tags" => vec!["a:test"]],
                log_event!["tags" => vec!["b:test"]],
            ),
            // Range - numeric, inclusive, unbounded (both), tag (negate).
            (
                "NOT a:[* TO *]",
                log_event!["tags" => vec!["b:test"]],
                log_event!["tags" => vec!["a:test"]],
            ),
            // Range - numeric, inclusive, unbounded (both), tag (negate w/-).
            (
                "-a:[* TO *]",
                log_event!["tags" => vec!["b:test"]],
                log_event!["tags" => vec!["a:test"]],
            ),
            // Range - numeric, inclusive, attribute.
            ("@b:[1 TO 10]", log_event!["b" => 5], log_event!["b" => 11]),
            (
                "@b:[1 TO 100]",
                log_event!["b" => "10"],
                log_event!["b" => "2"],
            ),
            // Range - numeric, inclusive, attribute (negate).
            (
                "NOT @b:[1 TO 10]",
                log_event!["b" => 11],
                log_event!["b" => 5],
            ),
            (
                "NOT @b:[1 TO 100]",
                log_event!["b" => "2"],
                log_event!["b" => "10"],
            ),
            // Range - numeric, inclusive, attribute (negate w/-).
            ("-@b:[1 TO 10]", log_event!["b" => 11], log_event!["b" => 5]),
            (
                "NOT @b:[1 TO 100]",
                log_event!["b" => "2"],
                log_event!["b" => "10"],
            ),
            // Range - alpha, inclusive, attribute.
            ("@b:[a TO z]", log_event!["b" => "c"], log_event!["b" => 5]),
            // Range - alphanumeric, inclusive, attribute.
            (
                r#"@b:["1" TO "100"]"#,
                log_event!["b" => "10"],
                log_event!["b" => "2"],
            ),
            // Range - alphanumeric, inclusive, attribute (negate).
            (
                r#"NOT @b:["1" TO "100"]"#,
                log_event!["b" => "2"],
                log_event!["b" => "10"],
            ),
            // Range - alphanumeric, inclusive, attribute (negate).
            (
                r#"-@b:["1" TO "100"]"#,
                log_event!["b" => "2"],
                log_event!["b" => "10"],
            ),
            // Range - tag, exclusive.
            (
                "f:{1 TO 100}",
                log_event!["tags" => vec!["f:10"]],
                log_event!["tags" => vec!["f:1"]],
            ),
            (
                "f:{1 TO 100}",
                log_event!["tags" => vec!["f:10"]],
                log_event!["tags" => vec!["f:100"]],
            ),
            // Range - tag, exclusive (negate).
            (
                "NOT f:{1 TO 100}",
                log_event!["tags" => vec!["f:1"]],
                log_event!["tags" => vec!["f:10"]],
            ),
            (
                "NOT f:{1 TO 100}",
                log_event!["tags" => vec!["f:100"]],
                log_event!["tags" => vec!["f:10"]],
            ),
            // Range - tag, exclusive (negate w/-).
            (
                "-f:{1 TO 100}",
                log_event!["tags" => vec!["f:1"]],
                log_event!["tags" => vec!["f:10"]],
            ),
            (
                "-f:{1 TO 100}",
                log_event!["tags" => vec!["f:100"]],
                log_event!["tags" => vec!["f:10"]],
            ),
            // Range - attribute, exclusive.
            ("@f:{1 TO 100}", log_event!["f" => 50], log_event!["f" => 1]),
            (
                "@f:{1 TO 100}",
                log_event!["f" => 50],
                log_event!["f" => 100],
            ),
            // Range - attribute, exclusive (negate).
            (
                "NOT @f:{1 TO 100}",
                log_event!["f" => 1],
                log_event!["f" => 50],
            ),
            (
                "NOT @f:{1 TO 100}",
                log_event!["f" => 100],
                log_event!["f" => 50],
            ),
            // Range - attribute, exclusive (negate w/-).
            (
                "-@f:{1 TO 100}",
                log_event!["f" => 1],
                log_event!["f" => 50],
            ),
            (
                "-@f:{1 TO 100}",
                log_event!["f" => 100],
                log_event!["f" => 50],
            ),
            // OR of two values
            (
                "@field:(value1 OR value2)",
                log_event!["field" => "value1"],
                log_event!["field" => "value"],
            ),
            // OR of two values
            (
                "@field:value1 OR @field:value2",
                log_event!["field" => "value1"],
                log_event!["field" => "value"],
            ),
            // negate OR of two values
            (
                "-@field:value1 OR -@field:value2",
                log_event!["field" => "value"],
                log_event!["field" => "value2"],
            ),
            // default AND of two values
            (
                "@field:value @field2:value2",
                log_event!["field" => "value", "field2" => "value2"],
                log_event!["field" => "value", "field2" => "value3"],
            ),
            // handles newline
            (
                "@field:(value1 OR \n value2)",
                log_event!["field" => "value1"],
                log_event!["field" => "value"],
            ),
        ]
    }

    /// Test a `Matcher` by providing a `Filter<V>` and a processor that receives an
    /// `Event`, and returns a `V`. This allows testing against the pass/fail events that are returned
    /// from `get_checks()` and modifying into a type that allows for their processing.
    fn test_filter<V, F, P>(filter: F, processor: P)
    where
        V: std::fmt::Debug + Send + Sync + Clone + 'static,
        F: Filter<V> + Resolver,
        P: Fn(Event) -> V,
    {
        let checks = get_checks();

        for (source, pass, fail) in checks {
            let node: QueryNode = source.parse().unwrap();
            let matcher = build_matcher(&node, &filter).unwrap();

            assert!(matcher.run(&processor(pass)));
            assert!(!matcher.run(&processor(fail)));
        }
    }

    #[test]
    /// Parse each Datadog Search Syntax query and check that it passes/fails.
    fn event_filter() {
        test_filter(EventFilter, |ev| ev.into_log())
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogSearchConfig>();
    }

    #[test]
    fn check_datadog() {
        for (source, pass, fail) in get_checks() {
            let config: DatadogSearchConfig = source.parse().unwrap();

            // Every query should build successfully.
            let cond = config
                .build(&Default::default())
                .unwrap_or_else(|_| panic!("build failed: {}", source));

            assert!(
                cond.check_with_context(pass.clone()).0.is_ok(),
                "should pass: {}\nevent: {}",
                source,
                serde_json::to_string(&pass.as_log()).unwrap(),
            );

            assert!(
                cond.check_with_context(fail.clone()).0.is_err(),
                "should fail: {}\nevent: {}",
                source,
                serde_json::to_string(&fail.as_log()).unwrap(),
            );
        }
    }
}
