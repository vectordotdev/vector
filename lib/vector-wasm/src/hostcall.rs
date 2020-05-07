use crate::Registration;
use snafu::Snafu;

#[derive(Debug, Snafu)]
#[repr(C)]
pub enum Error {
    #[snafu(display("Codec error: {}", source))]
    Codec { source: serde_json::error::Error },
    #[snafu(display("Null error: {}", source))]
    Nul { source: std::ffi::NulError },
    #[snafu(display("UTF-8 error: {}", source))]
    Utf8 { source: std::str::Utf8Error },
    #[snafu(display("Foreign Module error"))]
    Foreign,
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

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
pub fn emit(mut data: impl AsMut<[u8]>) {
    let data = data.as_mut();
    unsafe {
        ffi::emit(data.as_mut_ptr() as u64, data.len() as u64);
    }
    // No need to clean up manually. The slice is dropped.
}

pub mod ffi {
    extern "C" {
        pub(super) fn register(ptr: u64, size: u64);
        pub(super) fn emit(ptr: u64, size: u64);
    }
}
