#![deny(improper_ctypes)]

mod registration;
pub use registration::Registration;
mod role;
pub use role::Role;
pub mod hostcall;
pub mod interop;
