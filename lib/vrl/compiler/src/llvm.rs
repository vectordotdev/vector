use inkwell::{builder::Builder, module::Module};

static PRECOMPILED: &'static [u8] = include_bytes!(concat!(env!("OUT_DIR"), "/precompiled.bc"));

pub struct Context<'ctx> {
    context: &'ctx inkwell::context::Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
}
