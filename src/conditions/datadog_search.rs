use crate::conditions::{Condition, ConditionConfig, ConditionDescription, Conditional};
use datadog_filter::{
    fast_matcher::{self, Mode, Op},
    Resolver,
};
use datadog_search_syntax::{parse, Comparison, ComparisonValue, Field};
use regex::{bytes, Regex};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use vector_core::event::{Event, LogEvent, Value};

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub(crate) struct DatadogSearchConfig {
    source: String,
}

inventory::submit! {
    ConditionDescription::new::<DatadogSearchConfig>("datadog_search")
}

impl_generate_config_from_default!(DatadogSearchConfig);

/// Runner that contains the boxed `Matcher` function to check whether an `Event` matches
/// a Datadog Search Syntax query.
#[derive(Debug, Clone)]
pub struct DatadogSearchRunner {
    matcher: fast_matcher::FastMatcher,
}

impl Conditional for DatadogSearchRunner {
    fn check(&self, e: &Event) -> bool {
        if let Event::Log(log) = e {
            EventFilter::run(&self.matcher, log)
        } else {
            false
        }
    }
}

#[typetag::serde(name = "datadog_search")]
impl ConditionConfig for DatadogSearchConfig {
    fn build(&self, _enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        let node = parse(&self.source)?;
        let matcher = fast_matcher::build_matcher(&node, &EventFilter::default());

        Ok(Condition::DatadogSearch(DatadogSearchRunner { matcher }))
    }
}

//------------------------------------------------------------------------------

#[derive(Default, Clone)]
pub struct EventFilter;

impl EventFilter {
    pub fn run(matcher: &fast_matcher::FastMatcher, log: &LogEvent) -> bool {
        match &matcher.mode {
            Mode::One(op) => exec(&op, log),
            Mode::Any(ops) => ops.iter().any(|op| exec(op, log)),
            Mode::All(ops) => ops.iter().all(|op| exec(op, log)),
        }
    }
}

fn exec(op: &Op, log: &LogEvent) -> bool {
    match op {
        Op::True => true,
        Op::False => true,
        Op::Exists(field) => exists(field, log),
        Op::NotExists(field) => !exists(&field, log),
        Op::Equals { field, value } => match field {
            Field::Reserved(f) | Field::Facet(f) => equals(f, value, log),
            _ => false,
        },
        Op::TagExists(value) => tag_exists(value, log),
        Op::RegexMatch { field, re } => regex_match(field, re, log),
        Op::Prefix(field, value) => prefix(field, value, log),
        Op::Wildcard(field, value) => wildcard(field, value, log),
        Op::Compare(field, comparison, comparison_value) => {
            compare(field, *comparison, comparison_value, log)
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
                (ComparisonValue::Unbounded, ComparisonValue::Unbounded) => exists(field, log),
                // Unbounded lower.
                (ComparisonValue::Unbounded, _) => {
                    let op = if *upper_inclusive {
                        Comparison::Lte
                    } else {
                        Comparison::Lt
                    };
                    compare(field, op, upper, log)
                }
                // Unbounded upper.
                (_, ComparisonValue::Unbounded) => {
                    let op = if *lower_inclusive {
                        Comparison::Gte
                    } else {
                        Comparison::Gt
                    };

                    compare(field, op, lower, log)
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

                    compare(field, lower_op, lower, log) && compare(field, upper_op, upper, log)
                }
            }
        }
        Op::Not(matcher) => !EventFilter::run(matcher, log),
        Op::Nested(matcher) => EventFilter::run(matcher, log),
    }
}

fn exists(field: &Field, log: &LogEvent) -> bool {
    match field {
        Field::Tag(tag) => match log.get("tags") {
            Some(Value::Array(values)) => values
                .iter()
                .filter_map(|value| {
                    if let Value::Bytes(bytes) = value {
                        std::str::from_utf8(bytes).ok()
                    } else {
                        None
                    }
                })
                .any(|value| {
                    value == tag
                        || (value.starts_with(tag) && value.chars().nth(tag.len()) == Some(':'))
                }),
            _ => false,
        },
        // Literal field 'tags' needs to be compared by key.
        Field::Reserved(field) if field == "tags" => match log.get("tags") {
            Some(Value::Array(values)) => values
                .iter()
                .filter_map(|value| {
                    if let Value::Bytes(bytes) = value {
                        std::str::from_utf8(bytes).ok()
                    } else {
                        None
                    }
                })
                .any(|value| value == field),
            _ => false,
        },
        Field::Default(f) | Field::Facet(f) | Field::Reserved(f) => log.contains(&f),
    }
}

fn equals(field: &str, value: &str, log: &LogEvent) -> bool {
    match log.get(field) {
        Some(Value::Bytes(s)) => s == value.as_bytes(),
        _ => false,
    }
}

fn tag_exists(to_match: &str, log: &LogEvent) -> bool {
    match log.get("tags") {
        Some(Value::Array(values)) => values.iter().any(|value| {
            if let Value::Bytes(bytes) = value {
                bytes == to_match.as_bytes()
            } else {
                false
            }
        }),
        _ => false,
    }
}

fn regex_match(field: &str, re: &Regex, log: &LogEvent) -> bool {
    match log.get(field) {
        Some(Value::Bytes(s)) => {
            if let Some(s) = std::str::from_utf8(&s).ok() {
                re.is_match(s)
            } else {
                false
            }
        }
        _ => false,
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

/// Returns compiled wildcard regex.
#[must_use]
pub fn wildcard_regex(to_match: &str) -> bytes::Regex {
    bytes::Regex::new(&format!(
        "^{}$",
        regex::escape(to_match).replace("\\*", ".*")
    ))
    .expect("invalid wildcard regex")
}

fn prefix(field: &Field, pfx: &str, log: &LogEvent) -> bool {
    match field {
        // Default fields are matched by word boundary.
        Field::Default(field) => match log.get(field.as_str()) {
            Some(Value::Bytes(v)) => {
                let re = word_regex(&format!("{}*", pfx));
                re.is_match(&v)
            }
            _ => false,
        },
        // Tags are recursed until a match is found.
        Field::Tag(tag) => match log.get("tags") {
            Some(Value::Array(values)) => {
                let starts_with: String = format!("{}:{}", tag, pfx);
                values
                    .iter()
                    .any(|val: &Value| val.coerce_to_bytes().starts_with(starts_with.as_bytes()))
            }
            _ => false,
        },
        // All other field types are compared by complete value.
        Field::Reserved(field) | Field::Facet(field) => match log.get(field.as_str()) {
            Some(Value::Bytes(v)) => v.starts_with(pfx.as_bytes()),
            _ => false,
        },
    }
}

fn wildcard(field: &Field, wildcard: &str, log: &LogEvent) -> bool {
    match field {
        Field::Default(field) => match log.get(field.as_str()) {
            Some(Value::Bytes(v)) => {
                let re = word_regex(wildcard);
                re.is_match(&v)
            }
            _ => false,
        },
        Field::Tag(tag) => match log.get("tags") {
            Some(Value::Array(values)) => {
                let re = wildcard_regex(&format!("{}:{}", tag, wildcard));
                values
                    .iter()
                    .any(|val: &Value| re.is_match(&val.coerce_to_bytes()))
            }
            _ => false,
        },
        Field::Reserved(field) | Field::Facet(field) => match log.get(field.as_str()) {
            Some(Value::Bytes(v)) => {
                let re = wildcard_regex(wildcard);
                re.is_match(&v)
            }
            _ => false,
        },
    }
}

fn compare(
    field: &Field,
    comparator: Comparison,
    comparison_value: &ComparisonValue,
    log: &LogEvent,
) -> bool {
    let rhs = Cow::from(comparison_value.to_string());

    match field {
        // Facets are compared numerically if the value is numeric, or as strings otherwise.
        Field::Facet(f) => {
            match (log.get(&f), comparison_value) {
                // Integers.
                (Some(Value::Integer(lhs)), ComparisonValue::Integer(rhs)) => match comparator {
                    Comparison::Lt => lhs < rhs,
                    Comparison::Lte => lhs <= rhs,
                    Comparison::Gt => lhs > rhs,
                    Comparison::Gte => lhs >= rhs,
                },
                // Integer value - Float boundary
                (Some(Value::Integer(lhs)), ComparisonValue::Float(rhs)) => match comparator {
                    Comparison::Lt => (*lhs as f64) < *rhs,
                    Comparison::Lte => *lhs as f64 <= *rhs,
                    Comparison::Gt => *lhs as f64 > *rhs,
                    Comparison::Gte => *lhs as f64 >= *rhs,
                },
                // Floats.
                (Some(Value::Float(lhs)), ComparisonValue::Float(rhs)) => match comparator {
                    Comparison::Lt => lhs.into_inner() < *rhs,
                    Comparison::Lte => lhs.into_inner() <= *rhs,
                    Comparison::Gt => lhs.into_inner() > *rhs,
                    Comparison::Gte => lhs.into_inner() >= *rhs,
                },
                // Float value - Integer boundary
                (Some(Value::Float(lhs)), ComparisonValue::Integer(rhs)) => match comparator {
                    Comparison::Lt => lhs.into_inner() < *rhs as f64,
                    Comparison::Lte => lhs.into_inner() <= *rhs as f64,
                    Comparison::Gt => lhs.into_inner() > *rhs as f64,
                    Comparison::Gte => lhs.into_inner() >= *rhs as f64,
                },
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
        }
        // Tag values need extracting by "key:value" to be compared.
        Field::Tag(tag) => match log.get("tags") {
            Some(Value::Array(values)) => values.iter().any(|val: &Value| {
                match String::from_utf8_lossy(&val.coerce_to_bytes()).split_once(":") {
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
                }
            }),
            _ => false,
        },
        // All other tag types are compared by string.
        Field::Default(field) | Field::Reserved(field) => match log.get(field) {
            Some(Value::Bytes(lhs)) => {
                let rhs = rhs.as_bytes();
                match comparator {
                    Comparison::Lt => *lhs < rhs,
                    Comparison::Lte => *lhs <= rhs,
                    Comparison::Gt => *lhs > rhs,
                    Comparison::Gte => *lhs >= rhs,
                }
            }
            _ => false,
        },
    }
}

/// Uses the default `Resolver`, to build a `Vec<Field>`.
impl Resolver for EventFilter {}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;

    use crate::log_event;
    use datadog_filter::fast_matcher;
    use datadog_search_syntax::parse;
    use serde_json::json;
    use vector_core::event::Event;

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
            // Facet exists.
            (
                "_exists_:@b",
                log_event!["custom" => json!({"b": "foo"})],
                log_event!["custom" => json!({"a": "foo"})],
            ),
            // Facet exists (negate).
            (
                "NOT _exists_:@b",
                log_event!["custom" => json!({"a": "foo"})],
                log_event!["custom" => json!({"b": "foo"})],
            ),
            // Facet exists (negate w/-).
            (
                "-_exists_:@b",
                log_event!["custom" => json!({"a": "foo"})],
                log_event!["custom" => json!({"b": "foo"})],
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
            // Facet doesn't exist.
            (
                "_missing_:@b",
                log_event!["custom" => json!({"a": "foo"})],
                log_event!["custom" => json!({"b": "foo"})],
            ),
            // Facet doesn't exist (negate).
            (
                "NOT _missing_:@b",
                log_event!["custom" => json!({"b": "foo"})],
                log_event!["custom" => json!({"a": "foo"})],
            ),
            // Facet doesn't exist (negate w/-).
            (
                "-_missing_:@b",
                log_event!["custom" => json!({"b": "foo"})],
                log_event!["custom" => json!({"a": "foo"})],
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
                log_event!["custom" => json!({"a": "bla"})],
            ),
            // Quoted tag match (negate).
            (
                r#"NOT a:"bla""#,
                log_event!["custom" => json!({"a": "bla"})],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Quoted tag match (negate w/-).
            (
                r#"-a:"bla""#,
                log_event!["custom" => json!({"a": "bla"})],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Facet match.
            (
                "@a:bla",
                log_event!["custom" => json!({"a": "bla"})],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Facet match (negate).
            (
                "NOT @a:bla",
                log_event!["tags" => vec!["a:bla"]],
                log_event!["custom" => json!({"a": "bla"})],
            ),
            // Facet match (negate w/-).
            (
                "-@a:bla",
                log_event!["tags" => vec!["a:bla"]],
                log_event!["custom" => json!({"a": "bla"})],
            ),
            // Quoted facet match.
            (
                r#"@a:"bla""#,
                log_event!["custom" => json!({"a": "bla"})],
                log_event!["tags" => vec!["a:bla"]],
            ),
            // Quoted facet match (negate).
            (
                r#"NOT @a:"bla""#,
                log_event!["tags" => vec!["a:bla"]],
                log_event!["custom" => json!({"a": "bla"})],
            ),
            // Quoted facet match (negate w/-).
            (
                r#"-@a:"bla""#,
                log_event!["tags" => vec!["a:bla"]],
                log_event!["custom" => json!({"a": "bla"})],
            ),
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
            (
                "*b*la*",
                log_event!["custom" => json!({"title": "foobla"})],
                log_event![],
            ),
            // Multiple wildcards (negate).
            (
                "NOT *b*la*",
                log_event![],
                log_event!["custom" => json!({"title": "foobla"})],
            ),
            // Multiple wildcards (negate w/-).
            (
                "-*b*la*",
                log_event![],
                log_event!["custom" => json!({"title": "foobla"})],
            ),
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
                log_event!["custom" => r#"{"title": "foobla"}"#],
            ),
            // Multiple wildcards - tag (negate).
            (
                "NOT c:*b*la*",
                log_event!["custom" => r#"{"title": "foobla"}"#],
                log_event!["tags" => vec!["c:foobla"]],
            ),
            // Multiple wildcards - tag (negate w/-).
            (
                "-c:*b*la*",
                log_event!["custom" => r#"{"title": "foobla"}"#],
                log_event!["tags" => vec!["c:foobla"]],
            ),
            // Wildcard prefix - facet.
            (
                "@a:*bla",
                log_event!["custom" => json!({"a": "foobla"})],
                log_event!["tags" => vec!["a:foobla"]],
            ),
            // Wildcard prefix - facet (negate).
            (
                "NOT @a:*bla",
                log_event!["tags" => vec!["a:foobla"]],
                log_event!["custom" => json!({"a": "foobla"})],
            ),
            // Wildcard prefix - facet (negate w/-).
            (
                "-@a:*bla",
                log_event!["tags" => vec!["a:foobla"]],
                log_event!["custom" => json!({"a": "foobla"})],
            ),
            // Wildcard suffix - facet.
            (
                "@b:bla*",
                log_event!["custom" => json!({"b": "blabop"})],
                log_event!["tags" => vec!["b:blabop"]],
            ),
            // Wildcard suffix - facet (negate).
            (
                "NOT @b:bla*",
                log_event!["tags" => vec!["b:blabop"]],
                log_event!["custom" => json!({"b": "blabop"})],
            ),
            // Wildcard suffix - facet (negate w/-).
            (
                "-@b:bla*",
                log_event!["tags" => vec!["b:blabop"]],
                log_event!["custom" => json!({"b": "blabop"})],
            ),
            // Multiple wildcards - facet.
            (
                "@c:*b*la*",
                log_event!["custom" => json!({"c": "foobla"})],
                log_event!["tags" => vec!["c:foobla"]],
            ),
            // Multiple wildcards - facet (negate).
            (
                "NOT @c:*b*la*",
                log_event!["tags" => vec!["c:foobla"]],
                log_event!["custom" => json!({"c": "foobla"})],
            ),
            // Multiple wildcards - facet (negate w/-).
            (
                "-@c:*b*la*",
                log_event!["tags" => vec!["c:foobla"]],
                log_event!["custom" => json!({"c": "foobla"})],
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
            // Range - numeric, inclusive, facet.
            (
                "@b:[1 TO 10]",
                log_event!["custom" => json!({"b": 5})],
                log_event!["custom" => json!({"b": 11})],
            ),
            (
                "@b:[1 TO 100]",
                log_event!["custom" => json!({"b": "10"})],
                log_event!["custom" => json!({"b": "2"})],
            ),
            // Range - numeric, inclusive, facet (negate).
            (
                "NOT @b:[1 TO 10]",
                log_event!["custom" => json!({"b": 11})],
                log_event!["custom" => json!({"b": 5})],
            ),
            (
                "NOT @b:[1 TO 100]",
                log_event!["custom" => json!({"b": "2"})],
                log_event!["custom" => json!({"b": "10"})],
            ),
            // Range - numeric, inclusive, facet (negate w/-).
            (
                "-@b:[1 TO 10]",
                log_event!["custom" => json!({"b": 11})],
                log_event!["custom" => json!({"b": 5})],
            ),
            (
                "NOT @b:[1 TO 100]",
                log_event!["custom" => json!({"b": "2"})],
                log_event!["custom" => json!({"b": "10"})],
            ),
            // Range - alpha, inclusive, facet.
            (
                "@b:[a TO z]",
                log_event!["custom" => json!({"b": "c"})],
                log_event!["custom" => json!({"b": 5})],
            ),
            // Range - alphanumeric, inclusive, facet.
            (
                r#"@b:["1" TO "100"]"#,
                log_event!["custom" => json!({"b": "10"})],
                log_event!["custom" => json!({"b": "2"})],
            ),
            // Range - alphanumeric, inclusive, facet (negate).
            (
                r#"NOT @b:["1" TO "100"]"#,
                log_event!["custom" => json!({"b": "2"})],
                log_event!["custom" => json!({"b": "10"})],
            ),
            // Range - alphanumeric, inclusive, facet (negate).
            (
                r#"-@b:["1" TO "100"]"#,
                log_event!["custom" => json!({"b": "2"})],
                log_event!["custom" => json!({"b": "10"})],
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
            // Range - facet, exclusive.
            (
                "@f:{1 TO 100}",
                log_event!["custom" => json!({"f": 50})],
                log_event!["custom" => json!({"f": 1})],
            ),
            (
                "@f:{1 TO 100}",
                log_event!["custom" => json!({"f": 50})],
                log_event!["custom" => json!({"f": 100})],
            ),
            // Range - facet, exclusive (negate).
            (
                "NOT @f:{1 TO 100}",
                log_event!["custom" => json!({"f": 1})],
                log_event!["custom" => json!({"f": 50})],
            ),
            (
                "NOT @f:{1 TO 100}",
                log_event!["custom" => json!({"f": 100})],
                log_event!["custom" => json!({"f": 50})],
            ),
            // Range - facet, exclusive (negate w/-).
            (
                "-@f:{1 TO 100}",
                log_event!["custom" => json!({"f": 1})],
                log_event!["custom" => json!({"f": 50})],
            ),
            (
                "-@f:{1 TO 100}",
                log_event!["custom" => json!({"f": 100})],
                log_event!["custom" => json!({"f": 50})],
            ),
        ]
    }

    #[test]
    /// Parse each Datadog Search Syntax query and check that it passes/fails.
    fn event_filter() {
        for (source, pass, fail) in get_checks() {
            let node = parse(source).unwrap();
            let matcher = fast_matcher::build_matcher(&node, &EventFilter::default());

            assert!(
                EventFilter::run(&matcher, &pass.clone().into_log()),
                "should pass: {}\nevent: {:?}",
                source,
                pass.as_log()
            );
            assert!(
                !EventFilter::run(&matcher, &fail.clone().into_log()),
                "should fail: {}\nevent: {:?}",
                source,
                fail.as_log()
            );
        }
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogSearchConfig>();
    }

    #[test]
    fn check_datadog() {
        for (source, pass, fail) in get_checks() {
            let config = DatadogSearchConfig {
                source: source.to_owned(),
            };

            // Every query should build successfully.
            let cond = config
                .build(&Default::default())
                .unwrap_or_else(|_| panic!("build failed: {}", source));

            assert!(
                cond.check_with_context(&pass).is_ok(),
                "should pass: {}\nevent: {:?}",
                source,
                pass.as_log()
            );

            assert!(
                cond.check_with_context(&fail).is_err(),
                "should fail: {}\nevent: {:?}",
                source,
                fail.as_log()
            );
        }
    }
}
