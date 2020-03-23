use std::str::Bytes;
use serde_json::{Value, json};
use prost::Message;

pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

mod vector_api {
    use serde_json::Value;
    use std::str::{self, FromStr};
    use std::convert::TryFrom;
    use anyhow::Result;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    pub(crate) fn get(field: impl AsRef<str>) -> Result<Option<Value>> {
        let field_str = field.as_ref();
        let field_cstring = CString::new(field_str)?;
        let field_ptr: *const c_char = field_cstring.as_ptr();

        let hinted_value_len = unsafe {
            ffi::hint_field_length(
                field_ptr,
            )
        };

        if hinted_value_len == 0 {
            return Ok(None)
        }

        let mut value_buffer: Vec<c_char> = Vec::with_capacity(hinted_value_len);
        let mut value_buffer_ptr = value_buffer.as_mut_ptr();

        unsafe {
            ffi::get(
                field_ptr,
                value_buffer_ptr,
            )
        };

        let ret_cstring = unsafe { CString::from_raw(value_buffer_ptr) };
        let ret_str = ret_cstring.to_str()?;
        let ret_value: Value = serde_json::de::from_str(ret_str)?;

        Ok(Some(ret_value))
        // Ok(None)
    }

    pub(crate) fn insert(field: impl AsRef<str>, value: impl Into<Value>) -> Result<()> {
        let field_str = field.as_ref();
        let field_cstring = CString::new(field_str)?;
        println!("insert::field_cstring: {:?}", field_cstring);
        let field_ptr = field_cstring.as_ptr();

        let value = value.into();
        let value_serialized = serde_json::to_string(&value).unwrap();
        let value_cstring = CString::new(value_serialized)?;
        println!("insert::value_cstring: {:?}", value_cstring);
        let value_ptr = value_cstring.as_ptr();
        unsafe {
            Ok(ffi::insert(
                field_ptr,
                value_ptr,
            ))
        }
    }

    mod ffi {
        use std::os::raw::c_char;

        extern "C" {
            pub(super) fn get(
                field_ptr: *const c_char,
                output_ptr: *const c_char,
            ) -> usize;
            pub(super) fn insert(
                field_ptr: *const c_char,
                value_ptr: *const c_char,
            );
            pub(super) fn hint_field_length(
                field_ptr: *const c_char,
            ) -> usize;
        }
    }
}

#[no_mangle]
pub extern "C" fn process() -> bool {
    let result = vector_api::get("test");
    println!("From inside the wasm machine: {:?}", result);
    match result.unwrap() {
        Some(value) => {
            println!("Pre-insert");
            let decoded = crate::items::AddressBook::decode(value.as_str().expect("Protobuf field not a str").as_bytes()).unwrap();
            let reencoded = serde_json::to_string(&decoded).unwrap();
            vector_api::insert("processed", reencoded).unwrap();
            println!("Inserted");
            true
        },
        None => { println!("No result!"); false },
    };
    let result = vector_api::get("processed");
    println!("From inside the wasm machine (result): {:?}", result);
    true
}
