#![deny(improper_ctypes)]


use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(feature = "guest")]
pub mod guest;
#[cfg(feature = "host")]
pub mod host;

pub trait Role: Clone + Copy + Sized + Serialize + DeserializeOwned {}

pub mod roles {
    use super::Role;
    use serde::{Serialize, Deserialize};

    #[derive(Clone, Copy, Serialize, Deserialize, Default)]
    #[repr(C)]
    pub struct Sink { _dummy: u8, }
    impl Role for Sink {}
    #[derive(Clone, Copy, Serialize, Deserialize, Default)]
    #[repr(C)]
    pub struct Source { _dummy: u8, }
    impl Role for Source {}
    #[derive(Clone, Copy, Serialize, Deserialize, Default)]
    #[repr(C)]
    pub struct Transform { _dummy: u8, }
    impl Role for Transform {}
}
