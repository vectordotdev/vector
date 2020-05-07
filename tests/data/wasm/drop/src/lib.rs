use vector_wasm::Registration;

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
