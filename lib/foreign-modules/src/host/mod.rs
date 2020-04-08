//! The traits an implementor of the ForeignModule API is expected to implement.
//!
//!
//! Please ensure all your function signatures match these:
//!
//! ```rust
//! use std::os::raw::c_char;
//! use lucet_runtime::vmctx::Vmctx;
//! use lucet_runtime::lucet_hostcall;
//! use tracing::instrument;
//!
//! #[lucet_hostcall]
//! #[no_mangle]
//! #[instrument(skip(vmctx))]
//! unsafe fn hint_field_length(vmctx: &mut Vmctx, key_ptr: *const u8) -> usize {
//!     unimplemented!()
//! }
//!
//! #[lucet_hostcall]
//! #[no_mangle]
//! #[instrument(skip(vmctx))]
//! unsafe fn get(vmctx: &mut Vmctx, key_ptr: *const u8, value_ptr: *const u8) -> usize {
//!     unimplemented!()
//! }
//!
//! #[lucet_hostcall]
//! #[no_mangle]
//! #[instrument(skip(vmctx))]
//! unsafe fn insert(vmctx: &mut Vmctx, key_ptr: *const u8, value_ptr: *const u8) {
//!     unimplemented!()
//! }
//!
//! #[lucet_hostcall]
//! #[no_mangle]
//! #[instrument(skip(vmctx))]
//! unsafe fn register(vmctx: &mut Vmctx, registration_ptr: *const u8) {
//!     unimplemented!()
//! }
//! ```

