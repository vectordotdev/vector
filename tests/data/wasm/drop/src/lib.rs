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

#[no_mangle]
pub extern "C" fn allocate_buffer(bytes: u64) -> *mut u8 {
    let data: Vec<u8> = Vec::with_capacity(bytes as usize);
    let mut boxed = data.into_boxed_slice();
    boxed.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn drop_buffer(start: *mut u8, length: usize) {
    let _ = std::ptr::slice_from_raw_parts_mut(start, length);
}
