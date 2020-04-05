use std::os::raw::c_char;
use lucet_runtime::vmctx::Vmctx;
use lucet_runtime::lucet_hostcall;

#[lucet_hostcall]
#[no_mangle]
unsafe fn hint_field_length(vmctx: &mut Vmctx, key_ptr: *const c_char) -> usize {
    unimplemented!()
}

#[lucet_hostcall]
#[no_mangle]
unsafe fn get(vmctx: &mut Vmctx, key_ptr: *const c_char, value_ptr: *const c_char) -> usize {
    unimplemented!()
}

#[lucet_hostcall]
#[no_mangle]
unsafe fn insert(vmctx: &mut Vmctx, key_ptr: *const c_char, value_ptr: *const c_char) {
    unimplemented!()
}
