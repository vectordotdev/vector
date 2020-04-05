//! Ensure your hostcalls have this on them:
//!
//! ```rust
//! #[lucet_hostcall]
//! #[no_mangle]
//! ```

use super::context::ForeignModuleContext;
use lucet_runtime::{lucet_hostcall, vmctx::Vmctx};
use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::raw::c_char;
use std::str::FromStr;
use tracing::{event, Level};

#[lucet_hostcall]
#[no_mangle]
unsafe fn hint_field_length(vmctx: &mut Vmctx, key_ptr: *const c_char) -> usize {
    event!(Level::TRACE, "recieved hostcall");
    let hostcall_context = vmctx.get_embed_ctx_mut::<ForeignModuleContext>();
    let mut heap = vmctx.heap_mut();
    let field_cstr = CStr::from_ptr(heap[key_ptr as usize..].as_mut_ptr() as *mut c_char);
    let field_str = field_cstr.to_str().unwrap_or("Broke to str");
    let event = hostcall_context.event.as_ref().unwrap();

    let value = event.as_log().get(&field_str.into());
    let ret = match value {
        None => 0,
        Some(v) => {
            let serialized_value = serde_json::to_string(v).unwrap();
            let serialized_cstring = CString::new(serialized_value).unwrap();
            let serialized_bytes = serialized_cstring.into_bytes_with_nul();
            let len = serialized_bytes.len();
            event!(Level::TRACE, "hinting length {:#?}", len);
            len
        }
    };
    event!(Level::TRACE, "returning from hostcall");
    ret
}

#[lucet_hostcall]
#[no_mangle]
unsafe fn get(vmctx: &mut Vmctx, key_ptr: *const c_char, value_ptr: *const c_char) -> usize {
    event!(Level::TRACE, "recieved hostcall");
    let hostcall_context = vmctx.get_embed_ctx_mut::<ForeignModuleContext>();
    let mut heap = vmctx.heap_mut();

    let key_cstr = CStr::from_ptr(heap[key_ptr as usize..].as_mut_ptr() as *mut c_char);
    let key_str = key_cstr.to_str().unwrap_or("Broke to str");

    let event = hostcall_context.event.as_ref().unwrap();
    let maybe_value = event.as_log().get(&key_str.into());

    let ret = match maybe_value {
        None => 0,
        Some(v) => {
            let serialized_value = serde_json::to_string(v).unwrap();
            let serialized_cstring = CString::new(serialized_value).unwrap();
            let serialized_bytes = serialized_cstring.into_bytes_with_nul();
            let mut byte_slice = &mut heap[value_ptr as usize..];
            let wrote = byte_slice
                .write(serialized_bytes.as_ref())
                .expect("Write to known buffer failed.");
            wrote
        }
    };

    event!(Level::TRACE, "returning from hostcall");
    ret
}

#[lucet_hostcall]
#[no_mangle]
unsafe fn insert(vmctx: &mut Vmctx, key_ptr: *const c_char, value_ptr: *const c_char) {
    event!(Level::TRACE, "recieved hostcall");

    let mut hostcall_context = vmctx.get_embed_ctx_mut::<ForeignModuleContext>();
    let mut heap = vmctx.heap_mut();

    let key_cstr = CStr::from_ptr(heap[key_ptr as usize..].as_mut_ptr() as *mut c_char);
    let key_str = key_cstr.to_str().unwrap_or("Broke to str");

    let value_cstr = CStr::from_ptr(heap[value_ptr as usize..].as_mut_ptr() as *mut c_char);
    let value_str = value_cstr.to_str().unwrap_or("Broke to str");
    let value_val = serde_json::Value::from_str(value_str).unwrap_or("Broke on value into".into());

    let event = hostcall_context.event.as_mut().unwrap();
    event!(
        Level::TRACE,
        "inserting key {:?} with value {:?}",
        key_str,
        value_val
    );
    event.as_mut_log().insert(key_str, value_val);

    event!(Level::TRACE, "returning from hostcall");
}
