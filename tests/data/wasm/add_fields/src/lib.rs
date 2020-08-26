//! Add Fields
//!
//! A sample Vector WASM plugin.
//!
//! This plugin emulates the behavior of the `add_fields` transform from Vector.

#![deny(improper_ctypes)]
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryInto;
use vector_wasm::{hostcall, Registration, Role};
use once_cell::sync::OnceCell;

static FIELDS: OnceCell<HashMap<String, Value>> = OnceCell::new();

/// Perform one time initialization and registration.
/// 
/// During this time Vector and the plugin can validate that they can indeed work together,
/// do any one-time initialization, or validate configuration settings.
/// 
/// It's required that the plugin call [`vector_wasm::Registration::register`] before returning.
#[no_mangle]
pub extern "C" fn init() {
    // Vector provides you with a [`vector_wasm::WasmModuleConfig`] to validate for yourself.
    let config = hostcall::config().unwrap();
    assert_eq!(config.role, Role::Transform);

    // At this point, you should do any one-time initialization needed...
    FIELDS.set(config.options.into()).unwrap();

    // Finally, pass Vector a `vector_wasm::Registration`
    Registration::transform().register().unwrap();
}

/// Process data starting from a given point in memory to another point.
///
/// It's not necessary for the plugin to actually read, or parse this data.
///
/// Call [`vector_wasm::hostcall::emit`] to emit a message out.
///
/// # Returns
///
/// This function should return a hint of the number of emitted messages.
#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    // Vector allocates a chunk of memory through the hostcall interface.
    // You can view the data as a slice of bytes.
    let data = unsafe {
        std::ptr::slice_from_raw_parts_mut(data as *mut u8, length.try_into().unwrap())
            .as_mut()
            .unwrap()
    };

    // In this example, we're mutating the event to add fields. You do not need to mutate or even
    // parse the data provided.

    // For now, Vector uses simple JSON serialization to simplify bindings for other languages.
    //
    // **Please note that WASM support is still unstable!**
    //
    // We expect to alter this format in the future after some event data model improvements.
    let mut event: HashMap<String, Value> = serde_json::from_slice(data).unwrap();

    // You can mutate/reallocate freely. Vector plugins have a sandboxed heap, so
    // large storage sizes may require adjustments to the `max_heap_size` variable during module
    // registration.
    let fields = FIELDS.get().unwrap();
    for (key, value) in fields {
        event.insert(key.into(), value.clone().into());
    }

    // As with all data, it returns to bytes in the end.
    let output_buffer = serde_json::to_vec(&event).unwrap();

    // Emit the bytes back to Vector.
    hostcall::emit(output_buffer).unwrap();

    // Hint to Vector how many events you emitted.
    1
}

/// Perform one-time optional shutdown events.
///
/// **Note:** There is no guarantee this function will be called before shutdown,
/// as we may be forcibly killed.
#[no_mangle]
pub extern "C" fn shutdown() {}
