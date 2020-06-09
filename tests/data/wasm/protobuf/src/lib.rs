#![deny(improper_ctypes)]
use anyhow::{anyhow, Context, Result};
use prost::Message;
use serde_json::Value;
// This is **required**.
use std::convert::{TryFrom, TryInto};
pub use vector_wasm::interop::*;
use vector_wasm::{hostcall, Registration};

// Choose the output type here:
type DecodingTarget = crate::items::AddressBook;

// Match the proto structure here:
// Not sure what to add here? Check `target/wasm32-wasi-release/protobuf-*/out`
pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

// New WASM adventurers need not explore below unless they are curious souls.

fn handle(slice: &mut Vec<u8>) -> Result<Vec<u8>> {
    let mut json: Value = serde_json::from_slice(&slice).context("Vector sent invalid JSON")?;

    let obj = json
        .as_object_mut()
        .context("Vector provided a non-object input")?;

    let field = obj
        .remove("message")
        .ok_or_else(|| anyhow!("Could not retrieve message field."))
        .and_then(|v| {
            v.as_str()
                .context("Message field was not a string.")
                .map(str::as_bytes)
                .map(Vec::from)
                .or_else(|_| {
                    v.as_array()
                        .context("Message field was not an array or string.")
                        .and_then(|array| {
                            array
                                .iter()
                                .map(|maybe_number| {
                                    maybe_number
                                        .as_u64()
                                        .ok_or_else(|| {
                                            anyhow!("Could not retrieve message as number array.")
                                        })
                                        .and_then(|number| {
                                            u8::try_from(number).context(
                                            "Could not read an index of the number array as a u8.",
                                        )
                                        })
                                })
                                .collect::<anyhow::Result<Vec<u8>>>()
                        })
                })
        })?;

    let proto = DecodingTarget::decode(field.as_slice())
        .context("Message field did not contain protobuf")?;

    let value =
        serde_json::to_value(proto).context("Could not convert proto output to a JSON value")?;

    obj.insert("processed".into(), value);

    let buffer = serde_json::to_vec(&obj).context("Could not make JSON into bytes")?;

    Ok(buffer)
}

#[no_mangle]
pub extern "C" fn init() {
    let _config = hostcall::config().unwrap();
    Registration::transform().register().unwrap();
}

#[no_mangle]
pub extern "C" fn process(data: u32, length: u32) -> u32 {
    let mut buffer = unsafe {
        Vec::from_raw_parts(
            data as *mut u8,
            length.try_into().unwrap(),
            length.try_into().unwrap(),
        )
    };

    // At this point, if we have an error, we can only really panic.
    match handle(&mut buffer) {
        Err(e) => {
            // Output the error.
            hostcall::raise(e).unwrap();
            // Even in the case of failure, we emit the event so it can progress through the pipeline.
            hostcall::emit(&mut buffer).unwrap()
        }
        Ok(mut v) => {
            // Everything worked out, emit the event.
            hostcall::emit(&mut v).unwrap()
        }
    }
}

#[no_mangle]
pub extern "C" fn shutdown() {}

#[test]
fn fixture_test() {}
