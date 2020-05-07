use prost::Message;
use serde_json::Value;
use tracing::trace;
use vector_wasm::{hostcall, Registration};

// Choose the output type here:
type DecodingTarget = crate::items::AddressBook;

// Match the proto structure here:
pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

// You shouldn't need to alter the below.
fn raw_parts_to_slice(data: u64, length: u64) -> Option<&'static mut [u8]> {
    unsafe { std::ptr::slice_from_raw_parts_mut(data as *mut u8, length as usize).as_mut() }
}

#[no_mangle]
pub extern "C" fn init() {
    Registration::transform().register()
}

#[no_mangle]
pub extern "C" fn process(data: u64, length: u64) -> usize {
    // This code below is **particularly** defensive as we don't wantto lose anything or report
    // incorrect information.

    let slice = match raw_parts_to_slice(data, length) {
        Some(slice) => slice,
        None => {
            trace!("Vector sent an invalid slice.");
            return 0;
        }
    };

    let mut json: Value = match serde_json::from_slice(slice) {
        Ok(json) => json,
        Err(e) => {
            trace!(key = "message", error = ?e, "Vector sent invalid json");
            hostcall::emit(slice);
            // TODO: Emit
            return 0;
        }
    };

    let obj = match json.as_object_mut() {
        Some(obj) => obj,
        None => {
            trace!("Vector sent a non-object event");
            hostcall::emit(slice);
            return 0;
        }
    };

    let field = match obj.get("message").and_then(Value::as_str) {
        // Try to transform the proto into some json.
        Some(value) => value,
        None => {
            trace!(
                key = "message",
                "Expected key did not contain string to read as protobuf",
            );
            hostcall::emit(slice);
            return 0;
        }
    };

    let proto = match DecodingTarget::decode(field.as_bytes()) {
        Ok(proto) => proto,
        Err(e) => {
            trace!(
                key = "message",
                error = ?e,
                "Vector sent an event with a key containing a string, but it was not a protobuf",
            );
            hostcall::emit(slice);
            return 0;
        }
    };

    let value = match serde_json::to_value(proto) {
        Ok(proto) => proto,
        Err(e) => {
            trace!(error = ?e, "Failed to turn proto into valid JSON");
            hostcall::emit(slice);
            return 0;
        }
    };

    obj.insert("processed".into(), value);

    let buffer = match serde_json::to_vec(&obj) {
        Ok(proto) => proto,
        Err(e) => {
            trace!(error = ?e, "Could not turn JSON back into a buffer.");
            hostcall::emit(slice);
            return 0;
        }
    };

    hostcall::emit(buffer);
    1
}

#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}

#[no_mangle]
pub extern "C" fn allocate_buffer(bytes: u64) -> *mut u8 {
    let data: Vec<u8> = Vec::with_capacity(bytes as usize);
    let mut boxed = data.into_boxed_slice();
    boxed.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn drop_buffer(start: *mut u8, length: usize) {
    let _ = std::ptr::slice_from_raw_parts_mut(start, length);
}

#[test]
fn fixture_test() {}
