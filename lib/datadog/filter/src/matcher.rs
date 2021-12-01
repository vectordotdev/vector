use dyn_clone::{clone_trait_object, DynClone};
use std::{fmt, marker::PhantomData};

/// A `Matcher` is a type that contains a "run" method which returns true/false if value `T`
/// matches a filter.
pub trait Matcher<V>: DynClone + std::fmt::Debug + Send + Sync {
    fn run(&self, value: &V) -> bool;
}

clone_trait_object!(<V>Matcher<V>);

impl<V> Matcher<V> for bool {
    fn run(&self, _value: &V) -> bool {
        *self
    }
}

/// Container for holding a thread-safe function type that can receive a VRL
/// `Value`, and return true/false some internal expression.
#[derive(Clone)]
pub struct Run<V, T>
where
    V: Send + std::fmt::Debug + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    func: T,
    _phantom: PhantomData<V>,
}

impl<'a, V, T> Run<V, T>
where
    V: Send + std::fmt::Debug + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    pub fn boxed(func: T) -> Box<Self> {
        Box::new(Self {
            func,
            _phantom: PhantomData,
        })
    }
}

impl<V, T> Matcher<V> for Run<V, T>
where
    V: Send + fmt::Debug + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    fn run(&self, obj: &V) -> bool {
        (self.func)(obj)
    }
}

impl<'a, V, T> fmt::Debug for Run<V, T>
where
    V: Send + std::fmt::Debug + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Datadog matcher fn")
    }
}
