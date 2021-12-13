use inkwell::{builder::Builder, module::Module};

pub struct Context<'ctx> {
    context: &'ctx inkwell::context::Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
}
