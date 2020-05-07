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
pub fn emit(data: Vec<u8>) {
    let mut slice = data.into_boxed_slice();
    unsafe {
        ffi::emit(slice.as_mut_ptr() as u64, slice.len() as u64);
    }
}

pub mod ffi {
    extern "C" {
        pub(super) fn emit(ptr: u64, size: u64);
    }
}
