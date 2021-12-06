use datadog_filter::{Filter, Matcher, Resolver};
use datadog_search_syntax::{Comparison, ComparisonValue, Field};
use vector_core::event::Event;

#[derive(Default, Clone)]
pub struct EventFilter;

impl Resolver for EventFilter {}

impl Filter<Event> for EventFilter {
    fn exists(&self, field: Field) -> Box<dyn Matcher<Event>> {
        todo!()
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
