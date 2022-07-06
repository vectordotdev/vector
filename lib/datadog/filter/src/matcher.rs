use std::{fmt, marker::PhantomData};

use datadog_search_syntax::{BooleanType, QueryNode};
use dyn_clone::{clone_trait_object, DynClone};

use crate::{Filter, Resolver};

/// A `Matcher` is a type that contains a "run" method which returns true/false if value `V`
/// matches a filter.
pub trait Matcher<V>: DynClone + fmt::Debug + Send + Sync {
    fn run(&self, value: &V) -> bool;
}

clone_trait_object!(<V>Matcher<V>);

/// Implementing `Matcher` for bool allows a `Box::new(true|false)` convenience.
impl<V> Matcher<V> for bool {
    fn run(&self, _value: &V) -> bool {
        *self
    }
}

/// Container for holding a thread-safe function type that can receive a `V` value and
/// return true/false for whether the value matches some internal expectation.
#[derive(Clone)]
pub struct Run<V, T>
where
    V: fmt::Debug + Send + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    func: T,
    _phantom: PhantomData<V>, // Necessary to make generic over `V`.
}

impl<'a, V, T> Run<V, T>
where
    V: fmt::Debug + Send + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    /// Convenience for allocating a `Self`, which is generally how a `Run` is instantiated.
    pub fn boxed(func: T) -> Box<Self> {
        Box::new(Self {
            func,
            _phantom: PhantomData,
        })
    }
}

impl<V, T> Matcher<V> for Run<V, T>
where
    V: fmt::Debug + Send + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    /// Invokes the internal `func`, returning true if a value matches.
    fn run(&self, obj: &V) -> bool {
        (self.func)(obj)
    }
}

impl<'a, V, T> fmt::Debug for Run<V, T>
where
    V: fmt::Debug + Send + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Datadog matcher fn")
    }
}

/// Returns a closure that negates the value of the provided `Matcher`.
fn not<V>(matcher: Box<dyn Matcher<V>>) -> Box<dyn Matcher<V>>
where
    V: fmt::Debug + Send + Sync + Clone + 'static,
{
    Run::boxed(move |value| !matcher.run(value))
}

/// Returns a closure that returns true if any of the vector of `Matcher<V>` return true.
fn any<V>(matchers: Vec<Box<dyn Matcher<V>>>) -> Box<dyn Matcher<V>>
where
    V: fmt::Debug + Send + Sync + Clone + 'static,
{
    Run::boxed(move |value| matchers.iter().any(|func| func.run(value)))
}

/// Returns a closure that returns true if all of the vector of `Matcher<V>` return true.
fn all<V>(matchers: Vec<Box<dyn Matcher<V>>>) -> Box<dyn Matcher<V>>
where
    V: fmt::Debug + Send + Sync + Clone + 'static,
{
    Run::boxed(move |value| matchers.iter().all(|func| func.run(value)))
}

/// Build a filter by parsing a Datadog Search Syntax `QueryNode`, and invoking the appropriate
/// method on a `Filter` + `Resolver` implementation to determine the matching logic. Each method
/// returns a `Matcher<V>` which is intended to be invoked at runtime. `F` should implement both
/// `Fielder` + `Filter` in order to applying any required caching which may affect the operation
/// of a filter method. This function is intended to be used at boot-time and NOT in a hot path!
pub fn build_matcher<V, F>(node: &QueryNode, filter: &F) -> Box<dyn Matcher<V>>
where
    V: fmt::Debug + Send + Sync + Clone + 'static,
    F: Filter<V> + Resolver,
{
    match node {
        QueryNode::MatchNoDocs => Box::new(false),
        QueryNode::MatchAllDocs => Box::new(true),
        QueryNode::AttributeExists { attr } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| filter.exists(field))
                .collect();

            any(matchers)
        }
        QueryNode::AttributeMissing { attr } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| not(filter.exists(field)))
                .collect();

            all(matchers)
        }
        QueryNode::AttributeTerm { attr, value }
        | QueryNode::QuotedAttribute {
            attr,
            phrase: value,
        } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| filter.equals(field, value))
                .collect();

            any(matchers)
        }
        QueryNode::AttributePrefix { attr, prefix } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| filter.prefix(field, prefix))
                .collect();

            any(matchers)
        }
        QueryNode::AttributeWildcard { attr, wildcard } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| filter.wildcard(field, wildcard))
                .collect();

            any(matchers)
        }
        QueryNode::AttributeComparison {
            attr,
            comparator,
            value,
        } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| filter.compare(field, *comparator, value.clone()))
                .collect();

            any(matchers)
        }
        QueryNode::AttributeRange {
            attr,
            lower,
            lower_inclusive,
            upper,
            upper_inclusive,
        } => {
            let matchers = filter
                .build_fields(attr)
                .into_iter()
                .map(|field| {
                    filter.range(
                        field,
                        lower.clone(),
                        *lower_inclusive,
                        upper.clone(),
                        *upper_inclusive,
                    )
                })
                .collect();

            any(matchers)
        }
        QueryNode::NegatedNode { node } => not(build_matcher(node, filter)),
        QueryNode::Boolean { oper, nodes } => {
            let funcs = nodes
                .iter()
                .map(|node| build_matcher(node, filter))
                .collect();

            match oper {
                BooleanType::And => all(funcs),
                BooleanType::Or => any(funcs),
            }
        }
    }
}
