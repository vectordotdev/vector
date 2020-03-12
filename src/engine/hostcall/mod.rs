//! Ensure your hostcalls have this on them:
//!
//! ```rust
//! #[lucet_hostcall]
//! #[no_mangle]
//! ```

use super::context::EngineContext;
use lucet_runtime::{lucet_hostcall, vmctx::Vmctx};
use std::ffi::CString;
use std::slice;
use std::io::Write;
use toml::Value;
use std::convert::TryFrom;
use std::str::FromStr;

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn hint_field_length(vmctx: &mut Vmctx, field: *const u8, len: usize) -> usize {
    let mut hostcall_context = vmctx.get_embed_ctx_mut::<EngineContext>();

    let byte_slice = &vmctx.heap()[field as usize..field as usize + len];
    let field_str = std::str::from_utf8(byte_slice).expect("Didn't get UTF-8");

    let mut event = hostcall_context.event.as_ref().unwrap();
    let value = event.as_log().get(&field_str.clone().to_string().into());
    match value {
        None => 0,
        Some(v) => {
            let serialized_value = serde_json::to_string(v).unwrap();
            let len = serialized_value.as_bytes().len();
            println!("Hinting {:#?}", len);
            len
        },
    }
}

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn foo(vmctx: &mut Vmctx) {
    let mut hostcall_context = vmctx.get_embed_ctx_mut::<EngineContext>();
    println!("{:#?}", hostcall_context.event);
}

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn get(
    vmctx: &mut Vmctx,
    key: *const u8,
    key_len: usize,
    value_bytes: *const u8,
    value_len: usize,
) -> usize {
    let mut hostcall_context = vmctx.get_embed_ctx_mut::<EngineContext>();
    let mut heap = &mut vmctx.heap_mut();

    let byte_slice= &heap[key as usize..key as usize + key_len];
    let key_str = std::str::from_utf8(byte_slice).expect("Didn't get UTF-8");

    let mut event = hostcall_context.event.as_ref().unwrap();
    let maybe_value = event.as_log()
        .get(&key_str.clone().to_string().into());

    match maybe_value {
        None => 0,
        Some(v) => {
            let serialized_value = serde_json::to_string(v).unwrap();
            println!("Returning {:?} (into buffer of size {:#?})", serialized_value, value_len);
            let mut byte_slice = &mut heap[value_bytes as usize..value_bytes as usize + value_len];
            let wrote = byte_slice.write(serialized_value.as_bytes()).expect("Write to known buffer failed.");
            println!("Wrote bytes {:?}", wrote);
            wrote
        }
    }
}

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn insert(
    vmctx: &mut Vmctx,
    key: *const u8,
    key_len: usize,
    value: *const u8,
    value_len: usize,
) {
    let mut hostcall_context = vmctx.get_embed_ctx_mut::<EngineContext>();
    let mut heap = &mut vmctx.heap_mut();

    let key_slice= &heap[key as usize..key as usize + key_len];
    let key_str = std::str::from_utf8(key_slice).expect("Didn't get UTF-8");

    let value_slice= &heap[value as usize..value as usize + value_len];
    let value_str = std::str::from_utf8(&value_slice).expect("Didn't get UTF-8");
    let value = serde_json::Value::from_str(value_str).expect("Didn't get value");

    println!("Inserting {:?} (hinted len: {:#?})", value, value_len);

    let mut event = hostcall_context.event.as_mut().unwrap();
    event.as_mut_log().insert(key_str, value);
}
