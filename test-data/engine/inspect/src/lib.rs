#![no_std]

use core::prelude::*;
use core::panic::PanicInfo;

mod vector_api {
    pub(crate) fn foo() {
        unsafe { ffi::foo() }
    }

    mod ffi {
        extern "C" {
            pub(super) fn foo();
        }
    }
}



#[no_mangle]
pub extern "C" fn process() {
    vector_api::foo();
}

#[panic_handler]
fn panic(e: &PanicInfo) -> ! { unimplemented!() }
