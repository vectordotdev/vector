use dyn_clone::{clone_trait_object, DynClone};
use std::{fmt, marker::PhantomData};

/// A `Matcher` is a type that contains a "run" method which returns true/false if value `V`
/// matches a filter.
pub trait Matcher<V>: DynClone + std::fmt::Debug + Send + Sync {
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
    V: Send + std::fmt::Debug + Sync + Clone,
    T: Fn(&V) -> bool + Send + Sync + Clone,
{
    func: T,
    _phantom: PhantomData<V>, // Necessary to make generic over `V`.
}

impl<'a, V, T> Run<V, T>
where
    V: Send + std::fmt::Debug + Sync + Clone,
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
