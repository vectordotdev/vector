//! Writing a Foreign module guest involves writing some 'hooks' which the host will call over the
//! normal course of operation.
//!
//! Please ensure all your function signatures match these:
//!
//! ```rust
//! #[no_mangle]
//! pub extern "C" fn init(&mut self) -> Result<Option<AbstractEvent>, AbstractError>;
//! #[no_mangle]
//! pub extern "C" fn shutdown(&mut self) -> Result<(), AbstractError>;
//! #[no_mangle]
//! pub extern "C" fn process() -> bool {
//! ```


use crate::{Role, roles};

mod hostcall;

#[derive(Default)]
#[must_use]
pub struct Registration<R> where R: Role { role: R, }

impl<R> Registration<R> where R: Role {
    pub fn blanket_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
    /* ... */

    pub fn register(&mut self) -> Result<(), usize> { unimplemented!() }
}

impl Registration<roles::Transform> {
    pub fn transform_only_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
    /* ... */
}

impl Registration<roles::Sink> {
    pub fn sink_only_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
    /* ... */
}

impl Registration<roles::Source> {
    pub fn sink_only_option(&mut self, _: usize) -> &mut Self { unimplemented!() }
    /* ... */
}
