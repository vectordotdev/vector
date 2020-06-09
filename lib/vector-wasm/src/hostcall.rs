use crate::Registration;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fmt::Display;

/// Emit the data back to the host.
pub fn register(registration: &Registration) -> Result<()> {
    let buffer =
        serde_json::to_vec(registration).context("Could not turn registration to JSON.")?;
    let mut slice = buffer.into_boxed_slice();

    unsafe {
        ffi::register(slice.as_mut_ptr() as u32, slice.len() as u32);
    }

    Ok(())
}

/// Emit the data back to the host.
/// When returning `Ok(i64)` it indicates the number of events emitted so far.
pub fn emit(mut data: impl AsMut<[u8]>) -> Result<u32> {
    let data = data.as_mut();

    let retval = unsafe { ffi::emit(data.as_mut_ptr() as u32, data.len() as u32) };

    Ok(retval)
}

/// Emit the data back to the host.
pub fn raise(error: impl Display) -> Result<u32> {
    let mut string = format!("{}", error);
    let buffer = unsafe { string.as_mut_vec() };
    let parts = buffer.as_mut_slice();

    let retval = unsafe { ffi::raise(parts.as_mut_ptr() as u32, parts.len() as u32) };

    Ok(retval)
}

/// Retrieve the options from the instance context.
pub fn config() -> Result<HashMap<String, serde_json::Value>> {
    let size = unsafe { ffi::config_size() };
    let ptr = crate::interop::allocate_buffer(size);

    unsafe { ffi::config(ptr as u32, size) };

    let buffer = unsafe { Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize) };
    let config: HashMap<String, serde_json::Value> = serde_json::from_slice(&buffer)?;
    Ok(config)
}

pub mod ffi {
    extern "C" {
        pub(super) fn register(ptr: u32, size: u32);
        pub(super) fn emit(ptr: u32, size: u32) -> u32;
        pub(super) fn raise(ptr: u32, size: u32) -> u32;
        pub(super) fn config(ptr: u32, size: u32);
        pub(super) fn config_size() -> u32;
    }
}
