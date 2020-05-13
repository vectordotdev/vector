#![deny(improper_ctypes)]

use vector_wasm::Registration;
// This is **required**.
pub use vector_wasm::interop::*;

#[no_mangle]
pub extern "C" fn init() {
    Registration::transform().register()
}

#[no_mangle]
pub extern "C" fn process(_data: u64, _length: u64) -> usize {
    0
}

#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}
