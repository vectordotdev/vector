//! A common suite of structs, functions et al that are useful for the
//! benchmarking of vector transforms.
use vector::conditions::Condition;
use vector::event::Event;

#[derive(Debug, Clone)]
/// A struct that will always pass its check `Event`s.
pub struct AlwaysPass;

impl Condition for AlwaysPass {
    #[inline]
    fn check(&self, _event: &Event) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
/// A struct that will always fail its check `Event`s.
pub struct AlwaysFail;

impl Condition for AlwaysFail {
    #[inline]
    fn check(&self, _event: &Event) -> bool {
        false
    }
}
