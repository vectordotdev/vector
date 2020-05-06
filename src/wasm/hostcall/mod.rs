//! Ensure your hostcalls have this on them:
//!
//! ```rust
//! #[lucet_hostcall]
//! #[no_mangle]
//! ```

use super::context::EventBuffer;
use crate::event::LogEvent;
use crate::{internal_events, Event};
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
pub extern "C" fn emit(vmctx: &mut Vmctx, data: *mut u8, length: usize) -> usize {
    let internal_event = internal_events::Hostcall::begin(Role::Transform, "emit");

    let mut event_buffer = vmctx.get_embed_ctx_mut::<EventBuffer>();

    let mut heap = vmctx.heap_mut();
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

static HOSTCALL_API_INIT: Once = Once::new();

/// This is pretty hackish; we will hopefully be able to avoid this altogether once [this
/// issue](https://github.com/rust-lang/rust/issues/58037) is addressed.
#[no_mangle]
#[doc(hidden)]
pub extern "C" fn ensure_linked() {
    use std::ptr::read_volatile;
    HOSTCALL_API_INIT.call_once(|| unsafe {
        read_volatile(emit as *const extern "C" fn());
        lucet_wasi::export_wasi_funcs();
        lucet_runtime::lucet_internal_ensure_linked();
    });
}
