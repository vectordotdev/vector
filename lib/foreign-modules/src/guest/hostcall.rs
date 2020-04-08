use serde_json::Value;
use std::ffi::CString;
use std::os::raw::c_char;
use std::str;
use snafu::{Snafu, ResultExt};
use super::Registration;
use crate::{Role, roles};

#[derive(Debug, Snafu)]
#[repr(C)]
pub enum Error {
    #[snafu(display("Codec error: {}", source))]
    Codec { source: serde_json::error::Error, },
    #[snafu(display("Null error: {}", source))]
    Nul { source: std::ffi::NulError, },
    #[snafu(display("UTF-8 error: {}", source))]
    Utf8 { source: std::str::Utf8Error, },
    #[snafu(display("Foreign Module error"))]
    Foreign,
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Get a field from the event type.
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
    let field_ptr = field_cstring.as_ptr();

    let value = value.into();
    let value_serialized = serde_json::to_string(&value)
        .context(Codec)?;
    let value_cstring = CString::new(value_serialized)
        .context(Nul)?;

    let value_ptr = value_cstring.as_ptr();
    unsafe { Ok(ffi::insert(field_ptr, value_ptr)) }
}

pub fn register_transform(mut registration: Registration<roles::Transform>) -> Result<()> {
    let _result = unsafe { ffi::register_transform(&mut registration as *mut Registration<roles::Transform>) };
    Ok(())
}

pub fn register_sink(mut registration: Registration<roles::Sink>) -> Result<()> {
    let _result = unsafe { ffi::register_sink(&mut registration as *mut Registration<roles::Sink>) };
    Ok(())
}

pub fn register_source(mut registration: Registration<roles::Source>) -> Result<()> {
    let _result= unsafe { ffi::register_source(&mut registration as *mut Registration<roles::Source>) };
    Ok(())
}


pub mod ffi {
    use crate::guest::Registration;
    use crate::{Role, roles};
    use super::Result;

    #[must_use]
    #[repr(C)]
    pub enum FfiResult<T, E> {
        Ok(T),
        Err(E),
    }

    impl<T, E> Into<Result<T, super::Error>> for FfiResult<T, E> {
        fn into(self) -> Result<T, super::Error> {
            match self {
                FfiResult::Ok(t) => Ok(t),
                FfiResult::Err(e) => Err(super::Error::Foreign),
            }
        }
    }

    extern "C" {
        pub(super) fn get(field_ptr: *const i8, output_ptr: *const i8) -> usize;
        pub(super) fn insert(field_ptr: *const i8, value_ptr: *const i8);
        pub(super) fn hint_field_length(field_ptr: *const i8) -> usize;

        // Foreign items can't be generic, so we expose specialized ones.
        pub(super) fn register_transform(
            registration_ptr: *const Registration<roles::Transform>
        ) -> u32;
        pub(super) fn register_sink(
            registration_ptr: *const Registration<roles::Sink>
        ) -> u32;
        pub(super) fn register_source(
            registration_ptr: *const Registration<roles::Source>
        ) -> u32;
    }
}
