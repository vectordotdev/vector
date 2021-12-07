use datadog_filter::{Filter, Matcher, Resolver, Run};
use datadog_search_syntax::{Comparison, ComparisonValue, Field};
use vector_core::event::{Event, Value};

#[derive(Default, Clone)]
pub struct EventFilter;

/// Uses the default `Resolver`, to build a `Vec<Field>`.
impl Resolver for EventFilter {}

impl Filter<Event> for EventFilter {
    fn exists(&self, field: Field) -> Box<dyn Matcher<Event>> {
        match field {
            Field::Tag(tag) => {
                let starts_with = format!("{}:", tag);

                Run::boxed(move |ev: &Event| match ev {
                    Event::Log(log) => match log.get("tags") {
                        Some(Value::Array(v)) => v.iter().any(|v| {
                            let bytes = v.as_bytes();
                            let str_value = String::from_utf8_lossy(&bytes);

                            // The tag matches using either 'key' or 'key:value' syntax.
                            str_value == tag || str_value.starts_with(&starts_with)
                        }),
                        _ => false,
                    },
                    _ => false,
                })
            }
            // Literal field 'tags' needs to be compared by key.
            Field::Reserved(f) if f == "tags" => Run::boxed(move |ev| match ev {
                Event::Log(log) => match log.get(&f) {
                    Some(Value::Array(v)) => v.iter().any(|v| {
                        let bytes = v.as_bytes();
                        let str_value = String::from_utf8_lossy(&bytes);

                        str_value == f
                    }),
                    _ => false,
                },
                _ => false,
            }),
            Field::Default(f) | Field::Facet(f) | Field::Reserved(f) => {
                Run::boxed(move |ev| match ev {
                    Event::Log(log) => log.get(&f).is_some(),
                    _ => false,
                })
            }
        }
    }

    fn equals(&self, field: Field, to_match: &str) -> Box<dyn Matcher<Event>> {
        todo!()
    }

    fn prefix(&self, field: Field, prefix: &str) -> Box<dyn Matcher<Event>> {
        todo!()
    }

    fn wildcard(&self, field: Field, wildcard: &str) -> Box<dyn Matcher<Event>> {
        todo!()
    }

    fn compare(
        &self,
        field: Field,
        comparator: Comparison,
        comparison_value: ComparisonValue,
    ) -> Box<dyn Matcher<Event>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::EventFilter;
    use crate::test_util::*;

    use datadog_filter::build_matcher;
    use datadog_search_syntax::parse;

    #[test]
    /// Parse each Datadog Search Syntax query and check that it passes/fails.
    fn checks() {
        let checks = get_checks();
        let filter = EventFilter::default();

        for (source, pass, fail) in checks {
            let node = parse(source).unwrap();
            let matcher = build_matcher(&node, &filter);

            assert!(matcher.run(&pass));
            assert!(!matcher.run(&fail));
        }
    }
}
