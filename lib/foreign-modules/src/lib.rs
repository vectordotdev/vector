#![deny(improper_ctypes)]

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[cfg(feature = "guest")]
pub mod guest;
#[cfg(feature = "host")]
pub mod host;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub enum Role {
    Transform = 0,
    Source = 1,
    Sink = 2,
}

pub trait GuestPointer<Target, Pointer>: From<*mut Target>
where
    Target: Clone,
{
    fn deref(self, heap: &[u8]) -> Result<Target, std::ffi::NulError>;
}
