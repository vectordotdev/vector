//! Writing a Foreign module guest involves writing some 'hooks' which the host will call over the
//! normal course of operation.
//!
//! Please ensure all your function signatures match these:
//!
//! ```rust
//! #[no_mangle]
//! pub extern "C" fn init() -> Result<usize, usize> {}
//! #[no_mangle]
//! pub extern "C" fn shutdown() -> Result<usize, usize> {}
//! #[no_mangle]
//! pub extern "C" fn process() -> Result<usize, usize> {}
//! ```

use crate::{Role, roles};
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;

pub mod hostcall;

/// A module registration.
#[derive(Default, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Registration<R> where R: Role + Serialize + DeserializeOwned {
    #[serde(bound(deserialize = "R: DeserializeOwned"))]
    role: R,
    wasi: bool,
}

impl<R> Registration<R> where R: Role + Serialize + DeserializeOwned  {
    pub fn set_wasi(mut self, enabled: bool) -> Self {
        self.wasi = enabled;
        self
    }
}

impl Registration<roles::Transform> {
    pub fn register(self) -> Result<(), hostcall::Error> {
        hostcall::register_transform(self)
    }
}

impl Registration<roles::Sink> {
    pub fn register(self) -> Result<(), hostcall::Error> {
        hostcall::register_sink(self)
    }
}

impl Registration<roles::Source> {
    pub fn register(self) -> Result<(), hostcall::Error> {
        hostcall::register_source(self)
    }
}
