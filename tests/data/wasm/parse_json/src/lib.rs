//! Parse JSON
//!
//! A sample Vector WASM plugin for parsing JSON.
//!
//! This plugin emulates the behavior of the `json_parser` native transform for Vector as well as the
//! `parse_json` function in Vector Remap Language.

// Code comments have been removed from this module. The `add_fields` Wasm function is documented
// pretty thoroughly in case you need insight into what's going on here :)

#![deny(improper_ctypes)]
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryInto;
use vector_wasm::{hostcall, Registration, Role};

#[no_mangle]
pub extern "C" fn init() {
    let config = hostcall::config().unwrap();
    assert_eq!(config.role, Role::Transform);

    Registration::transform().register().unwrap();
}

#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
            .as_mut()
            .unwrap()
    };

    // Perform the initial JSON parsing required to access the event, i.e.:
    // {"message": "...", "timestamp": "..."}
    let initial_event: HashMap<String, Value> = serde_json::from_slice(data).unwrap();

    let log_message = initial_event.get("message").unwrap().to_string();

    // This parses the extracted log message, which is assumed to be a valid JSON string, into JSON
    let parsed_json: serde_json::Value = serde_json::from_str(&log_message).unwrap();

    // Covert the output into bytes
    let output = serde_json::to_vec(&parsed_json).unwrap();

    hostcall::emit(output).unwrap();

    1
}

#[no_mangle]
pub extern "C" fn shutdown() {}
