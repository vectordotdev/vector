use serde_json::Value;
use snafu::{ResultExt, Snafu};
use std::{ffi::CString, os::raw::c_char, str};

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

/// Get a field from the event type.
pub fn get(field: impl AsRef<str>) -> Result<Option<Value>> {
    let field_str = field.as_ref();
    let field_cstring = CString::new(field_str).context(Nul)?;
    let field_ptr: *const c_char = field_cstring.as_ptr();

    let hinted_value_len = unsafe { ffi::hint_field_length(field_ptr) };

    if hinted_value_len == 0 {
        return Ok(None);
    }

    let mut value_buffer: Vec<c_char> = Vec::with_capacity(hinted_value_len);
    let value_buffer_ptr = value_buffer.as_mut_ptr();

    unsafe { ffi::get(field_ptr, value_buffer_ptr) };

    let ret_cstring = unsafe { CString::from_raw(value_buffer_ptr) };
    let ret_str = ret_cstring.to_str().context(Utf8)?;
    let ret_value: Value = serde_json::de::from_str(ret_str).context(Codec)?;

    Ok(Some(ret_value))
    // Ok(None)
}

pub fn insert(field: impl AsRef<str>, value: impl Into<Value>) -> Result<()> {
    let field_str = field.as_ref();
    let field_cstring = CString::new(field_str).context(Nul)?;
    let field_ptr = field_cstring.as_ptr();

    let value = value.into();
    let value_serialized = serde_json::to_string(&value).context(Codec)?;
    let value_cstring = CString::new(value_serialized).context(Nul)?;

    let value_ptr = value_cstring.as_ptr();
    unsafe { Ok(ffi::insert(field_ptr, value_ptr)) }
}

pub mod ffi {
    extern "C" {
        pub(super) fn get(field_ptr: *const i8, output_ptr: *const i8) -> usize;
        pub(super) fn insert(field_ptr: *const i8, value_ptr: *const i8);
        pub(super) fn hint_field_length(field_ptr: *const i8) -> usize;
    }
}
