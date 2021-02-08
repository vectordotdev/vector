//! VRL Announcement Example
//!
//! A WASM implementation of syslog parsing for benchmarks

#![deny(improper_ctypes)]
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryInto;
use vector_wasm::{hostcall, Registration, Role};

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

    // For now, Vector uses simple JSON serialization to simplify bindings for other languages.
    //
    // **Please note that WASM support is still unstable!**
    //
    // We expect to alter this format in the future after some event data model improvements.
    let mut event: HashMap<String, Value> = serde_json::from_slice(data).unwrap();

    // The following is equivalent to the remap script:
    // . = parse_syslog!(.message)

    let message = event.remove("message");

    let message = message
        .as_ref()
        .map(|v| v.as_str().unwrap_or_default())
        .unwrap_or_default();

    let parsed = syslog_loose::parse_message(message);

    event.insert("message".to_owned(), parsed.msg.into());

    if let Some(host) = parsed.hostname {
        event.insert("hostname".to_owned(), host.to_string().into());
    }
    if let Some(facility) = parsed.facility {
        event.insert("facility".to_owned(), facility.as_str().to_owned().into());
    }
    if let Some(severity) = parsed.severity {
        event.insert("severity".to_owned(), severity.as_str().to_owned().into());
    }
    if let syslog_loose::Protocol::RFC5424(version) = parsed.protocol {
        event.insert("version".to_owned(), (version as i64).into());
    }
    if let Some(app_name) = parsed.appname {
        event.insert("appname".to_owned(), app_name.to_owned().into());
    }
    if let Some(msg_id) = parsed.msgid {
        event.insert("msgid".to_owned(), msg_id.to_owned().into());
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            syslog_loose::ProcId::PID(pid) => pid.into(),
            syslog_loose::ProcId::Name(name) => name.to_string().into(),
        };
        event.insert("procid".to_owned(), value);
    }

    for element in parsed.structured_data.into_iter() {
        for (name, value) in element.params.into_iter() {
            let key = format!("{}.{}", element.id, name);
            event.insert(key, value.to_owned().into());
        }
    }

    if let Some(timestamp) = parsed.timestamp {
        event.insert("timestamp".to_owned(), timestamp.to_rfc3339().into());
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
