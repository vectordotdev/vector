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

use crate::Role;
use serde::{Deserialize, Serialize};

pub mod hostcall;

/// A module registration.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Registration {
    role: Role,
    wasi: bool,
}

impl Registration {
    pub fn transform() -> Self {
        Self {
            role: Role::Transform,
            wasi: Default::default(),
        }
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn set_wasi(mut self, enabled: bool) -> Self {
        self.wasi = enabled;
        self
    }
    pub fn wasi(&self) -> bool {
        self.wasi
    }
}
