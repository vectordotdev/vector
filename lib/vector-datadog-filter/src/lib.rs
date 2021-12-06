use datadog_filter::{Filter, Matcher, Resolver, Run};
use datadog_search_syntax::{Comparison, ComparisonValue, Field};
use std::borrow::{Borrow, Cow};
use vector_core::event::{Event, Value};

#[derive(Default, Clone)]
pub struct EventFilter;

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
            _ => todo!(),
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
#[macro_export]
macro_rules! log_event {
    ($($key:expr => $value:expr),*  $(,)?) => {
        #[allow(unused_variables)]
        {
            let mut event = Event::Log(vector_core::event::LogEvent::default());
            let log = event.as_mut_log();
            $(
                log.insert($key, $value);
            )*
            event
        }
    };
}

#[cfg(test)]
pub fn get_checks() -> Vec<(&'static str, Event, Event)> {
    vec![
        // Tag exists.
        (
            "_exists_:a",                        // Source
            log_event!["tags" => vec!["a:foo"]], // Pass
            log_event!["tags" => vec!["b:foo"]], // Fail
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use datadog_filter::build_matcher;
    use datadog_search_syntax::parse;

    #[test]
    fn bla() {
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
