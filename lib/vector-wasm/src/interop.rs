#[no_mangle]
pub extern "C" fn allocate_buffer(bytes: u64) -> *mut u8 {
    let mut data: Vec<u8> = Vec::with_capacity(bytes as usize);
    let ptr = data.as_mut_ptr();
    std::mem::forget(data); // Yes this is unsafe, we'll get it back later.
    ptr
}

#[no_mangle]
pub extern "C" fn drop_buffer(start: *mut u8, length: usize) {
    let _ = unsafe { Vec::from_raw_parts(start, length, length) };
}
