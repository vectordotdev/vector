use super::{Matcher, Run};
use datadog_search_syntax::{Comparison, ComparisonValue, Field};
use dyn_clone::{clone_trait_object, DynClone};
use std::fmt::Debug;

/// A `Filter` is a generic type that contains methods that are invoked by the `build_filter`
/// function. Each method returns a heap-allocated `Matcher<V>` (typically a closure) containing
/// logic to determine whether the value matches the filter. A filter is intended to be side-effect
/// free and idempotent, and so only receives an immutable reference to self.
pub trait Filter<V: Debug + Send + Sync + Clone + 'static>: DynClone {
    /// Determine whether a field value exists.
    fn exists(&self, field: Field) -> Box<dyn Matcher<V>>;

    /// Determine whether a field value equals `to_match`.
    fn equals(&self, field: Field, to_match: &str) -> Box<dyn Matcher<V>>;

    /// Determine whether a value starts with a prefix.
    fn prefix(&self, field: Field, prefix: &str) -> Box<dyn Matcher<V>>;

    /// Determine whether a value matches a wilcard.
    fn wildcard(&self, field: Field, wildcard: &str) -> Box<dyn Matcher<V>>;

    /// Compare a field value against `comparison_value`, using one of the `comparator` operators.
    fn compare(
        &self,
        field: Field,
        comparator: Comparison,
        comparison_value: ComparisonValue,
    ) -> Box<dyn Matcher<V>>;

    /// Determine whether a field value falls within a range. By default, this will use
    /// `self.compare` on both the lower and upper bound.
    fn range(
        &self,
        field: Field,
        lower: ComparisonValue,
        lower_inclusive: bool,
        upper: ComparisonValue,
        upper_inclusive: bool,
    ) -> Box<dyn Matcher<V>> {
        match (&lower, &upper) {
            // If both bounds are wildcards, just check that the field exists to catch the
            // special case for "tags".
            (ComparisonValue::Unbounded, ComparisonValue::Unbounded) => self.exists(field),
            // Unbounded lower.
            (ComparisonValue::Unbounded, _) => {
                let op = if upper_inclusive {
                    Comparison::Lte
                } else {
                    Comparison::Lt
                };

                self.compare(field, op, upper)
            }
            // Unbounded upper.
            (_, ComparisonValue::Unbounded) => {
                let op = if lower_inclusive {
                    Comparison::Gte
                } else {
                    Comparison::Gt
                };

                self.compare(field, op, lower)
            }
            // Definitive range.
            _ => {
                let lower_op = if lower_inclusive {
                    Comparison::Gte
                } else {
                    Comparison::Gt
                };

                let upper_op = if upper_inclusive {
                    Comparison::Lte
                } else {
                    Comparison::Lt
                };

                let lower_func = self.compare(field.clone(), lower_op, lower);
                let upper_func = self.compare(field, upper_op, upper);

                Run::boxed(move |value| lower_func.run(value) && upper_func.run(value))
            }
        }
    }
}

clone_trait_object!(<V>Filter<V>);
