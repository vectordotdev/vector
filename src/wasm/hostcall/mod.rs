//! Ensure your hostcalls have this on them:
//!
//! ```rust
//! #![lucet_hostcall]
//! #![no_mangle]
//! ```

use super::context::EventBuffer;
use crate::wasm::context::RaisedError;
use crate::wasm::WasmModuleConfig;
use crate::{internal_events, Event};
use lucet_runtime::{lucet_hostcall, vmctx::Vmctx};
use std::sync::Once;
use vector_wasm::{Registration, Role};

#[lucet_hostcall]
#[no_mangle]
pub extern "C" fn register(vmctx: &mut Vmctx, data: u64, length: u64) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "register");

    let heap = vmctx.heap_mut();
    let slice = &heap[data as usize..(length as usize + data as usize)];
    let value: Registration = serde_json::from_slice(slice).unwrap();

    let mut maybe_registration = vmctx.get_embed_ctx_mut::<Option<Registration>>();
    *maybe_registration = Some(value);

    internal_event.complete();
    0
}

#[lucet_hostcall]
#[no_mangle]
pub extern "C" fn emit(vmctx: &mut Vmctx, data: u64, length: u64) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "emit");

    let mut event_buffer = vmctx.get_embed_ctx_mut::<EventBuffer>();

    let heap = vmctx.heap_mut();
    let slice = &heap[data as usize..(length as usize + data as usize)];

    // TODO: Add some usability around `LogEvent` for this.
    let value: serde_json::Value = serde_json::from_slice(slice).unwrap();
    let mut event = Event::new_empty_log();
    for (key, value) in value.as_object().unwrap() {
        event.as_mut_log().insert(key, value.clone());
    }

    event_buffer.push_back(event);

    internal_event.complete();
    0
}

#[lucet_hostcall]
#[no_mangle]
pub extern "C" fn raise(vmctx: &mut Vmctx, data: u64, length: u64) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "raise");

    let heap = vmctx.heap_mut();
    let slice = &heap[data as usize..(length as usize + data as usize)];

    let value = String::from_utf8(slice.into()).unwrap();

    let mut maybe_error = vmctx.get_embed_ctx_mut::<RaisedError>();
    maybe_error.error = Some(value);

    internal_event.complete();
    0
}

#[lucet_hostcall]
#[no_mangle]
pub extern "C" fn config_size(vmctx: &mut Vmctx) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "config_size");

    let config = vmctx.get_embed_ctx::<WasmModuleConfig>();
    let buf = serde_json::to_vec(&*config).unwrap();
    let length = buf.len();

    internal_event.complete();
    length
}

#[lucet_hostcall]
#[no_mangle]
pub extern "C" fn config(vmctx: &mut Vmctx, buffer: u64, length: u64) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "config");

    let config = vmctx.get_embed_ctx::<WasmModuleConfig>();
    let buf = serde_json::to_vec(&*config).unwrap();

    let mut heap = vmctx.heap_mut();
    let slice = &mut heap[buffer as usize..(length as usize + buffer as usize)];

    slice.copy_from_slice(buf.as_ref());

    internal_event.complete();
    0
}

static HOSTCALL_API_INIT: Once = Once::new();

/// This is pretty hackish; we will hopefully be able to avoid this altogether once [this
/// issue](https://github.com/rust-lang/rust/issues/58037) is addressed.
#[no_mangle]
#[doc(hidden)]
pub extern "C" fn ensure_linked() {
    use std::ptr::read_volatile;
    HOSTCALL_API_INIT.call_once(|| unsafe {
        read_volatile(emit as *const extern "C" fn());
        read_volatile(raise as *const extern "C" fn());
        read_volatile(config as *const extern "C" fn());
        read_volatile(config_size as *const extern "C" fn());
        lucet_wasi::export_wasi_funcs();
        lucet_runtime::lucet_internal_ensure_linked();
    });
}
