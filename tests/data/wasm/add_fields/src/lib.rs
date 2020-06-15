#![deny(improper_ctypes)]

use serde_json::Value;
use std::collections::BTreeMap;
use vector_wasm::{hostcall, Registration};
// This is **required**.
use std::convert::TryInto;
pub use vector_wasm::interop::*;

#[no_mangle]
pub extern "C" fn init() {
    let _config = hostcall::config().unwrap();
    Registration::transform().register().unwrap();
}

#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
            .as_mut()
            .unwrap()
    };
    let mut event: BTreeMap<String, Value> = serde_json::from_slice(data).unwrap();
    event.insert("new_field".into(), "new_value".into());
    event.insert("new_field_2".into(), "new_value_2".into());
    hostcall::emit(serde_json::to_vec(&event).unwrap()).unwrap();
    1
}

#[no_mangle]
pub extern "C" fn shutdown() {}
