#![deny(improper_ctypes)]


use serde::de::DeserializeOwned;
use serde::{Serialize, Deserialize};

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
