#![deny(improper_ctypes)]

use serde::{Deserialize, Serialize};
pub mod hostcall;

/// Denotes the intended role of the module.
///
/// This type is used as part of the [`Registration`](guest::Registration) process.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub enum Role {
    /// A transform.
    Transform = 0,
    /// A source.
    Source = 1,
    /// A sink.
    Sink = 2,
}

/// A pointer into a guest.
///
/// Allows the host to deref the pointer given the guest's heap.
pub trait GuestPointer<Target, Pointer>: From<*mut Target>
where
    Target: Clone,
{
    /// Dereference the pointer inside of some heap.
    fn deref(self, heap: &[u8]) -> Result<Target, std::ffi::NulError>;
}


/// A module registration.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Registration {
    /// The role of the module.
    role: Role,
    /// If this module requires WASI.
    ///
    /// WASI significantly increases module requirements, but enables the WebAssembly System
    /// Interface.
    ///
    /// * Enabled, the guest can be built with `wasi32-wasi` targets and Rust's `stdlib`.
    /// * Disabled, `#![no_std]` applications
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

