//! Ensure your hostcalls have this on them:
//!
//! ```rust
//! #[lucet_hostcall]
//! #[no_mangle]
//! ```

use super::context::EngineContext;
use lucet_runtime::{lucet_hostcall, vmctx::Vmctx};

#[lucet_hostcall]
#[no_mangle]
pub fn foo(vmctx: &mut Vmctx) {
    let mut hostcall_context = vmctx.get_embed_ctx_mut::<EngineContext>();
    println!("{:#?}", hostcall_context.events);
}
