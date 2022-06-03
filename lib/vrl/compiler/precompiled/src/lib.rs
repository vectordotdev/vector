use std::{alloc::Layout, collections::HashMap};

pub static LLVM_BITCODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/precompiled.bc"));

extern "Rust" {
    fn __rust_alloc(layout: Layout) -> *mut u8;

    fn __rust_alloc_zeroed(layout: Layout) -> *mut u8;

    fn __rust_dealloc(ptr: *mut u8, layout: Layout);

    fn __rust_realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8;

    fn __rust_alloc_error_handler(size: usize, align: usize) -> !;

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn __rust_probestack();

    #[cfg(target_os = "macos")]
    // https://github.com/rust-lang/rust/issues/59164
    fn __emutls_get_address(_control: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

// https://github.com/rust-lang/rust/issues/47384
pub fn symbols() -> HashMap<&'static str, usize> {
    let mut symbols = HashMap::<&str, usize>::new();
    symbols.insert("__rust_alloc", __rust_alloc as usize);
    symbols.insert("__rust_alloc_zeroed", __rust_alloc_zeroed as usize);
    symbols.insert("__rust_dealloc", __rust_dealloc as usize);
    symbols.insert("__rust_realloc", __rust_realloc as usize);
    symbols.insert("__rust_alloc_error_handler", __rust_realloc as usize);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    symbols.insert("__rust_probestack", __rust_probestack as usize);
    #[cfg(target_os = "linux")]
    {
        symbols.insert("fstat64", libc::fstat64 as usize);
        symbols.insert("fstatat64", libc::fstatat64 as usize);
        symbols.insert("lstat64", libc::lstat64 as usize);
        symbols.insert("stat64", libc::stat64 as usize);
    }
    #[cfg(target_os = "macos")]
    symbols.insert("__emutls_get_address", __emutls_get_address as usize);
    symbols
}
