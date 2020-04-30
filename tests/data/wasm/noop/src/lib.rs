use vector_wasm::Registration;

#[no_mangle]
pub extern "C" fn init() -> *mut Registration {
    &mut Registration::transform().set_wasi(true) as *mut Registration
}

#[no_mangle]
pub extern "C" fn process() -> usize {
    0
}

#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}
