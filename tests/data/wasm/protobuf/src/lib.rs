#![deny(improper_ctypes)]
use anyhow::{Context, Result};
use prost::Message;
use serde_json::Value;
use vector_wasm::{hostcall, Registration};

// Choose the output type here:
type DecodingTarget = crate::items::AddressBook;

// Match the proto structure here:
pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

fn handle(slice: &mut Vec<u8>) -> Result<Vec<u8>> {
    let mut json: Value = serde_json::from_slice(&slice).context("Vector sent invalid JSON")?;

    let obj = json
        .as_object_mut()
        .context("Vector provided a non-object input")?;

    let field = obj
        .get("message")
        .and_then(Value::as_str)
        .context("Vector sent a log without a message")?;

    let proto = DecodingTarget::decode(field.as_bytes())
        .context("Message field did not contain protobuf")?;

    let value =
        serde_json::to_value(proto).context("Could not convert proto output to a JSON value")?;

    obj.insert("processed".into(), value);

    let buffer = serde_json::to_vec(&obj).context("Could not make JSON into bytes")?;

    Ok(buffer)
}

#[no_mangle]
pub extern "C" fn init() {
    Registration::transform().register()
}

#[no_mangle]
pub extern "C" fn process(data: u64, length: u64) -> i64 {
    let mut buffer =
        unsafe { Vec::from_raw_parts(data as *mut u8, length as usize, length as usize) };

    match handle(&mut buffer) {
        Err(e) => {
            hostcall::emit(&mut buffer).unwrap();
            hostcall::raise(e).unwrap();
            drop(buffer);
            1
        }
        Ok(mut v) => {
            hostcall::emit(&mut v).unwrap();
            drop(v);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}

#[no_mangle]
pub extern "C" fn allocate_buffer(bytes: u64) -> *mut u8 {
    let mut data: Vec<u8> = Vec::with_capacity(bytes as usize);
    let ptr = data.as_mut_ptr();
    std::mem::forget(data); // Yes this is unsafe, we'll get it back later.
    ptr
}

#[no_mangle]
pub extern "C" fn drop_buffer(start: *mut u8, length: usize) {
    let _ = std::ptr::slice_from_raw_parts_mut(start, length);
}

#[test]
fn fixture_test() {}
