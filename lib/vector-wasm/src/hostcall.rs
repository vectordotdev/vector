use crate::Registration;
use anyhow::Result;

/// Emit the data back to the host.
pub fn register(registration: &Registration) {
    // TODO: Better error handling.
    let buffer = serde_json::to_vec(registration).unwrap();
    let mut slice = buffer.into_boxed_slice();
    unsafe {
        ffi::register(slice.as_mut_ptr() as u64, slice.len() as u64);
    }
    // No need to clean up manually. The slice is dropped.
}

/// Emit the data back to the host.
pub fn emit(mut data: impl AsMut<[u8]>) -> Result<i64> {
    let data = data.as_mut();
    let retval = unsafe { ffi::emit(data.as_mut_ptr() as u64, data.len() as u64) };
    Ok(retval)
}

/// Emit the data back to the host.
pub fn raise(error: anyhow::Error) -> Result<i64> {
    let mut string = format!("{}", error);
    let buffer = unsafe { string.as_mut_vec() };
    let parts = buffer.as_mut_slice();
    let retval = unsafe { ffi::raise(parts.as_mut_ptr() as u64, parts.len() as u64) };
    drop(parts);
    // No need to clean up manually. The slice is dropped.
    Ok(retval)
}

pub mod ffi {
    extern "C" {
        pub(super) fn register(ptr: u64, size: u64);
        pub(super) fn emit(ptr: u64, size: u64) -> i64;
        pub(super) fn raise(ptr: u64, size: u64) -> i64;
    }
}
