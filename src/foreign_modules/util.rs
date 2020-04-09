use foreign_modules::guest::Registration;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

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

impl<Target> From<*mut Registration> for GuestPointer<Target>
where
    Target: Clone,
{
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
}
