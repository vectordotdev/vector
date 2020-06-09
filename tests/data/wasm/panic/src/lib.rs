#![deny(improper_ctypes)]

use vector_wasm::{hostcall, Registration};
// This is **required**.
pub use vector_wasm::interop::*;

#[no_mangle]
pub extern "C" fn init() {
    let _config = hostcall::config().unwrap();
    Registration::transform().register().unwrap();
}

#[no_mangle]
pub extern "C" fn process(_data: u32, _length: u32) -> u32 {
    panic!("At the disco");
}

#[no_mangle]
pub extern "C" fn shutdown() {}
