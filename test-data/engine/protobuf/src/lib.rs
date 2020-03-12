use std::str::Bytes;
use serde_json::Value;

pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

mod vector_api {
    use serde_json::Value;
    use std::str::{self, FromStr};
    use std::convert::TryFrom;

    pub(crate) fn get(field: impl AsRef<[u8]>) -> Option<Value> {
        let field_bytes = field.as_ref();

        let hinted_value_len = unsafe {
            ffi::hint_field_length(
                field_bytes.as_ptr(),
                field_bytes.len())
        };
        let mut value_buffer = Vec::with_capacity(hinted_value_len);

        unsafe {
            ffi::get(
                field_bytes.as_ptr(),
                field_bytes.len(),
                value_buffer.as_mut_slice().as_mut_ptr(),
                value_buffer.capacity())
        };

        let value_str = std::str::from_utf8(&value_buffer).expect("Didn't get UTF-8");
        Some(serde_json::Value::from_str(value_str).expect("Didn't get value"))
    }

    pub(crate) fn insert(field: impl AsRef<[u8]>, value: impl Into<Value>) {
        let field_bytes = field.as_ref();
        let value = value.into();
        let value_string = value.to_string();
        let value_bytes = value_string.as_bytes();

        unsafe {
            ffi::insert(
                field_bytes.as_ptr(),
                field_bytes.len(),
                value_bytes.as_ptr(),
                value_bytes.len())
        };
    }

    mod ffi {
        extern "C" {
            pub(super) fn get(
                field_ptr: *const u8,
                field_len: usize,
                output_ptr: *const u8,
                output_len: usize,
            ) -> usize;
            pub(super) fn insert(
                field_ptr: *const u8,
                field_len: usize,
                value_ptr: *const u8,
                value_len: usize,
            );
            pub(super) fn hint_field_length(
                field_ptr: *const u8,
                field_len: usize,
            ) -> usize;
        }
    }
}

#[no_mangle]
pub extern "C" fn process() -> bool {
    let result = vector_api::get("test");
    match result {
        Some(value) => {
            vector_api::insert("processed", value);
            true
        },
        None => false
    }
}
