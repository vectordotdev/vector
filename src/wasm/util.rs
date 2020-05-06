use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use vector_wasm::Registration;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(transparent)]
/// A pointer into the guest VM.
pub struct GuestPointer<Target>
where
    Target: Clone,
{
    guest_pointer: usize,
    target: PhantomData<Target>,
}

impl From<*mut Registration> for GuestPointer<Registration> {
    fn from(guest_pointer: *mut Registration) -> Self {
        Self {
            guest_pointer: guest_pointer as usize,
            target: Default::default(),
        }
    }
}

impl<Target> GuestPointer<Target>
where
    Target: Clone,
{
    pub(crate) fn deref(self, heap: &mut [u8]) -> Result<Target, std::ffi::NulError> {
        let host_pointer = heap[self.guest_pointer..].as_mut_ptr() as *const Target;
        Ok(unsafe { (*host_pointer).clone() })
    }
    pub(crate) fn guest_pointer(&mut self) -> usize {
        self.guest_pointer
    }
}
