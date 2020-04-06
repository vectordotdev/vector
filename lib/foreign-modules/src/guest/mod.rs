//! Writing a Foreign module guest involves writing some 'hooks' which the host will call over the
//! normal course of operation.
//!
//! Please ensure all your function signatures match these:
//!
//! ```rust
//! #[no_mangle]
//! pub extern "C" fn init(&mut self) -> Result<Option<AbstractEvent>, AbstractError>;
//! #[no_mangle]
//! pub extern "C" fn shutdown(&mut self) -> Result<(), AbstractError>;
//! #[no_mangle]
//! pub extern "C" fn process() -> bool {
//! ```

use serde_json::Value;
use std::ffi::CString;
use std::os::raw::c_char;
use std::str;
use snafu::{Snafu, ResultExt};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Codec error: {}", source))]
    Codec { source: serde_json::error::Error, },
    #[snafu(display("Null error: {}", source))]
    Nul { source: std::ffi::NulError, },
    #[snafu(display("UTF-8 error: {}", source))]
    Utf8 { source: std::str::Utf8Error, },
}

type Result<T, E = Error> = std::result::Result<T, E>;

pub fn get(field: impl AsRef<str>) -> Result<Option<Value>> {
    let field_str = field.as_ref();
    let field_cstring = CString::new(field_str)
        .context(Nul)?;
    let field_ptr: *const c_char = field_cstring.as_ptr();

    let hinted_value_len = unsafe { ffi::hint_field_length(field_ptr) };

    if hinted_value_len == 0 {
        return Ok(None);
    }

    let mut value_buffer: Vec<c_char> = Vec::with_capacity(hinted_value_len);
    let value_buffer_ptr = value_buffer.as_mut_ptr();

    unsafe { ffi::get(field_ptr, value_buffer_ptr) };

    let ret_cstring = unsafe { CString::from_raw(value_buffer_ptr) };
    let ret_str = ret_cstring.to_str()
        .context(Utf8)?;
    let ret_value: Value = serde_json::de::from_str(ret_str)
        .context(Codec)?;

    Ok(Some(ret_value))
    // Ok(None)
}

pub fn insert(field: impl AsRef<str>, value: impl Into<Value>) -> Result<()> {
    let field_str = field.as_ref();
    let field_cstring = CString::new(field_str)
        .context(Nul)?;
    println!("insert::field_cstring: {:?}", field_cstring);
    let field_ptr = field_cstring.as_ptr();

    let value = value.into();
    let value_serialized = serde_json::to_string(&value)
        .context(Codec)?;
    let value_cstring = CString::new(value_serialized)
        .context(Nul)?;
    println!("insert::value_cstring: {:?}", value_cstring);
    let value_ptr = value_cstring.as_ptr();
    unsafe { Ok(ffi::insert(field_ptr, value_ptr)) }
}

mod ffi {
    use std::os::raw::c_char;

    extern "C" {
        pub(super) fn get(field_ptr: *const c_char, output_ptr: *const c_char) -> usize;
        pub(super) fn insert(field_ptr: *const c_char, value_ptr: *const c_char);
        pub(super) fn hint_field_length(field_ptr: *const c_char) -> usize;
    }
}
