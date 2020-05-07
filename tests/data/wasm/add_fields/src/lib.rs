use serde_json::Value;
use std::collections::BTreeMap;
use vector_wasm::{hostcall, Registration};

#[no_mangle]
pub extern "C" fn init() {
    Registration::transform().register()
}

#[no_mangle]
pub extern "C" fn process(data: u64, length: u64) -> usize {
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length as usize)
            .as_mut()
            .unwrap()
    };
    let mut event: BTreeMap<String, Value> = serde_json::from_slice(data).unwrap();
    event.insert("new_field".into(), "new_value".into());
    event.insert("new_field_2".into(), "new_value_2".into());
    hostcall::emit(serde_json::to_vec(&event).unwrap());
    1
}
#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}
