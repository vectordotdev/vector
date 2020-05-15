use crate::Registration;
use anyhow::Result;
use std::collections::HashMap;
use std::fmt::Display;

/// Emit the data back to the host.
pub fn register(registration: &Registration) -> Result<()> {
    let buffer = serde_json::to_vec(registration).context("Could not turn registration to JSON.");
    let mut slice = buffer.into_boxed_slice();
    unsafe {
        ffi::register(slice.as_mut_ptr() as u64, slice.len() as u64);
    }
    Ok(())
}

/// Emit the data back to the host.
/// When returning `Ok(i64)` it indicates the number of events emitted so far.
pub fn emit(mut data: impl AsMut<[u8]>) -> Result<i64> {
    let data = data.as_mut();
    let retval = unsafe { ffi::emit(data.as_mut_ptr() as u64, data.len() as u64) };
    Ok(retval)
}

/// Emit the data back to the host.
pub fn raise(error: impl Display) -> Result<()> {
    let mut string = format!("{}", error);
    let buffer = unsafe { string.as_mut_vec() };
    let parts = buffer.as_mut_slice();
    // TODO: Check for `-1`.
    let _ = unsafe { ffi::raise(parts.as_mut_ptr() as u64, parts.len() as u64) };
    Ok(())
}

/// Retrieve the options from the instance context.
pub fn config() -> Result<HashMap<String, serde_json::Value>> {
    let size = unsafe { ffi::config_size() };
    let ptr = crate::interop::allocate_buffer(size);
    // TODO: Check for `-1`.
    let _ = unsafe { ffi::config(ptr as u64, size) };
    let buffer = unsafe { Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize) };
    let config: HashMap<String, serde_json::Value> = serde_json::from_slice(&buffer)?;
    Ok(config)
}

pub mod ffi {
    extern "C" {
        pub(super) fn register(ptr: u64, size: u64);
        pub(super) fn emit(ptr: u64, size: u64) -> i64;
        pub(super) fn raise(ptr: u64, size: u64) -> i64;
        pub(super) fn config(ptr: u64, size: u64) -> u64;
        pub(super) fn config_size() -> u64;
    }
}
