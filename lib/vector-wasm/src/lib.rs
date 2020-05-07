#![deny(improper_ctypes)]

mod registration;
pub use registration::Registration;
mod role;
pub use role::Role;
pub mod hostcall;

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
