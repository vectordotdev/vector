//! Ensure your hostcalls have this on them:
//!
//! ```rust
//! #[lucet_hostcall]
//! #[no_mangle]
//! ```

use super::context::EventBuffer;
use crate::internal_events;
use lucet_runtime::{lucet_hostcall, vmctx::Vmctx};
use std::{
    ffi::{CStr, CString},
    io::Write,
    os::raw::c_char,
    str::FromStr,
    sync::Once,
};
use vector_wasm::Role;

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn hint_field_length(vmctx: &mut Vmctx, key_ptr: *const c_char) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "hint_field_length");

    let hostcall_context = vmctx.get_embed_ctx_mut::<EventBuffer>();
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
            len
        }
    };

    internal_event.complete();
    ret
}

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn get(vmctx: &mut Vmctx, key_ptr: *const c_char, value_ptr: *const c_char) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "get");

    let hostcall_context = vmctx.get_embed_ctx_mut::<EventBuffer>();
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

    internal_event.complete();
    ret
}

#[lucet_hostcall]
#[no_mangle]
pub unsafe fn insert(vmctx: &mut Vmctx, key_ptr: *const c_char, value_ptr: *const c_char) {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "insert");

    let mut hostcall_context = vmctx.get_embed_ctx_mut::<EventBuffer>();
    let mut heap = vmctx.heap_mut();

    let key_cstr = CStr::from_ptr(heap[key_ptr as usize..].as_mut_ptr() as *mut c_char);
    let key_str = key_cstr.to_str().unwrap_or("Broke to str");

    let value_cstr = CStr::from_ptr(heap[value_ptr as usize..].as_mut_ptr() as *mut c_char);
    let value_str = value_cstr.to_str().unwrap_or("Broke to str");
    let value_val = serde_json::Value::from_str(value_str).unwrap_or("Broke on value into".into());

    let event = hostcall_context.event.as_mut().unwrap();

    event.as_mut_log().insert(key_str, value_val);

    internal_event.complete();
}

static HOSTCALL_API_INIT: Once = Once::new();

/// This is pretty hackish; we will hopefully be able to avoid this altogether once [this
/// issue](https://github.com/rust-lang/rust/issues/58037) is addressed.
#[no_mangle]
#[doc(hidden)]
pub extern "C" fn ensure_linked() {
    use std::ptr::read_volatile;
    HOSTCALL_API_INIT.call_once(|| unsafe {
        read_volatile(hint_field_length as *const extern "C" fn());
        read_volatile(get as *const extern "C" fn());
        read_volatile(insert as *const extern "C" fn());
        lucet_wasi::export_wasi_funcs();
        lucet_runtime::lucet_internal_ensure_linked();
    });
}
