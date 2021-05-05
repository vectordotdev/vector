//! Hostcall endpoints exposed to guests.
use super::context::EventBuffer;
use crate::event::Event;
use crate::wasm::context::RaisedError;
use crate::wasm::WasmModuleConfig;
use lucet_runtime::vmctx::Vmctx;
use std::convert::TryInto;
use vector_wasm::Registration;
pub use wrapped_for_ffi::ensure_linked;

// Also add any new functions to the `ffi::ensure_linked` function!
pub const HOSTCALL_LIST: [&str; 5] = ["emit", "register", "raise", "config_size", "config"];

pub fn emit(vmctx: &Vmctx, data: u32, length: u32) -> crate::Result<u32> {
    let mut event_buffer = vmctx.get_embed_ctx_mut::<EventBuffer>();
    let heap = vmctx.heap_mut();
    let slice = &heap[data as usize..(length as usize + data as usize)];

    // TODO: Add some usability around `LogEvent` for this.
    let value: serde_json::Value = serde_json::from_slice(slice)?;
    let mut event = Event::new_empty_log();
    for (key, value) in value.as_object().ok_or("Passed JSON was not object.")? {
        event.as_mut_log().insert(key, value.clone());
    }

    event_buffer.push_back(event);
    Ok(event_buffer.events.len().try_into()?)
}

fn register(vmctx: &Vmctx, data: u32, length: u32) -> crate::Result<()> {
    let heap = vmctx.heap_mut();
    let slice = &heap[data as usize..(length as usize + data as usize)];
    let value: Registration = serde_json::from_slice(slice).unwrap();

    let mut maybe_registration = vmctx.get_embed_ctx_mut::<Option<Registration>>();
    *maybe_registration = Some(value);

    Ok(())
}

fn raise(vmctx: &Vmctx, data: u32, length: u32) -> crate::Result<u32> {
    let heap = vmctx.heap_mut();
    let slice = &heap[data as usize..(length as usize + data as usize)];

    let value = String::from_utf8(slice.into())?;

    let mut maybe_error = vmctx.get_embed_ctx_mut::<RaisedError>();
    maybe_error.error = Some(value);
    Ok(if maybe_error.error.is_some() { 1 } else { 0 })
}

fn config_size(vmctx: &Vmctx) -> crate::Result<u32> {
    let config = vmctx.get_embed_ctx::<WasmModuleConfig>();
    let buf = serde_json::to_vec(&*config)?;
    Ok(buf.len().try_into()?)
}

fn config(vmctx: &Vmctx, buffer: u32, length: u32) -> crate::Result<()> {
    let config = vmctx.get_embed_ctx::<WasmModuleConfig>();
    let buf = serde_json::to_vec(&*config)?;

    let mut heap = vmctx.heap_mut();
    let slice = &mut heap[buffer as usize..(length as usize + buffer as usize)];

    slice.copy_from_slice(buf.as_ref());
    Ok(())
}

/// All functions here must be fully C ABI compatible for wasm32-wasi.
mod wrapped_for_ffi {
    use crate::internal_events;
    use lucet_runtime::{lucet_hostcall, vmctx::Vmctx};
    use std::sync::Once;
    use vector_wasm::Role;

    static HOSTCALL_API_INIT: Once = Once::new();

    /// This is pretty hackish; we will hopefully be able to avoid this altogether once [this
    /// issue](https://github.com/rust-lang/rust/issues/58037) is addressed.
    #[no_mangle]
    #[doc(hidden)]
    pub extern "C" fn ensure_linked() {
        use std::ptr::read_volatile;
        // Also add any new functions to the `super::HOSTCALL_LIST` const!
        HOSTCALL_API_INIT.call_once(|| unsafe {
            read_volatile(emit as *const extern "C" fn());
            read_volatile(raise as *const extern "C" fn());
            read_volatile(config as *const extern "C" fn());
            read_volatile(config_size as *const extern "C" fn());
            lucet_wasi::export_wasi_funcs();
            lucet_runtime::lucet_internal_ensure_linked();
        });
    }

    #[lucet_hostcall]
    #[no_mangle]
    pub extern "C" fn register(vmctx: &Vmctx, data: u32, length: u32) {
        let internal_event =
            internal_events::WasmHostcallProgress::begin(Role::Transform, "register");
        match super::register(vmctx, data, length) {
            Ok(_) => internal_event.complete(),
            Err(error) => {
                internal_event.error(format!("{}", error));
            }
        }
    }

    #[lucet_hostcall]
    #[no_mangle]
    pub extern "C" fn emit(vmctx: &Vmctx, data: u32, length: u32) -> u32 {
        let internal_event = internal_events::WasmHostcallProgress::begin(Role::Transform, "emit");
        match super::emit(vmctx, data, length) {
            Ok(retval) => {
                internal_event.complete();
                retval
            }
            Err(error) => {
                internal_event.error(format!("{}", error));
                0
            }
        }
    }

    #[lucet_hostcall]
    #[no_mangle]
    pub extern "C" fn raise(vmctx: &Vmctx, data: u32, length: u32) -> u32 {
        let internal_event = internal_events::WasmHostcallProgress::begin(Role::Transform, "raise");
        match super::raise(vmctx, data, length) {
            Ok(retval) => {
                internal_event.complete();
                retval
            }
            Err(e) => {
                internal_event.error(format!("{}", e));
                0
            }
        }
    }

    #[lucet_hostcall]
    #[no_mangle]
    pub extern "C" fn config_size(vmctx: &Vmctx) -> u32 {
        let internal_event =
            internal_events::WasmHostcallProgress::begin(Role::Transform, "config_size");
        match super::config_size(vmctx) {
            Ok(retval) => {
                internal_event.complete();
                retval
            }
            Err(e) => {
                internal_event.error(format!("{}", e));
                0
            }
        }
    }

    #[lucet_hostcall]
    #[no_mangle]
    pub extern "C" fn config(vmctx: &Vmctx, buffer: u32, length: u32) {
        let internal_event =
            internal_events::WasmHostcallProgress::begin(Role::Transform, "config");
        match super::config(vmctx, buffer, length) {
            Ok(retval) => {
                internal_event.complete();
                retval
            }
            Err(e) => {
                internal_event.error(format!("{}", e));
            }
        }
    }
}
