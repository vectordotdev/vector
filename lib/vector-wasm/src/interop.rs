use std::convert::TryInto;

#[no_mangle]
pub extern "C" fn allocate_buffer(bytes: u32) -> u32 {
    // These are u32->u32 casts that should never fail.
    let mut data: Vec<u8> = Vec::with_capacity(bytes.try_into().unwrap());
    let ptr = data.as_mut_ptr();
    std::mem::forget(data); // Yes this is unsafe, we'll get it back later.
    ptr as u32
}

#[no_mangle]
pub extern "C" fn drop_buffer(start: *mut u8, length: u32) {
    // These are u32->u32 casts that should never fail.
    let _ = unsafe {
        Vec::from_raw_parts(
            start,
            length.try_into().unwrap(),
            length.try_into().unwrap(),
        )
    };
}
